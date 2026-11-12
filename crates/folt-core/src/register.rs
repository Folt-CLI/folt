//! Explicit-path registration. Planning reads metadata and a bounded Markdown
//! prefix. Apply atomically writes missing folio-name keys before hashing and
//! ingesting those bytes, and registers the batch in one transaction.
//!
//! Ordinary failures restore rewritten files if still unchanged, and remove this
//! call's markers and Folio directories. Restores that fail before publication
//! report that the file keeps its name; published restores report durability errors.
//! Writes sync a temporary file and its directory, change the inode,
//! and refuse hard links; permissions are copied, but ownership/ACLs/xattrs are not.
//! A crash before the transaction can leave names in files and orphan directories.
//! A fresh plan reads those names on retry; discovery/repair owns the orphans,
//! and retry creates new Folios rather than adopting them.
use crate::config::limits::SCAN_WINDOW;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::config::env::Environment;
use crate::config::formats::OBJECT_ALGORITHM;
use crate::config::limits::SUMMARY_LARGEST;
use crate::config::names::{DEFAULT_BRANCH, MARKDOWN_EXTENSIONS, MARKER_FILE};
use crate::config::settings::Settings;
use crate::folio_dir::{Durability, FolioDirError, FolioSpec, create_folio};
use crate::folio_name::{FolioName, FolioNameError, StemError};
use crate::front_matter::{self, Edit, FrontMatterError};
use crate::home::config_file::ConfigError;
use crate::home::{Home, HomeError, HomeLock};
use crate::id::FolioId;
use crate::layout::FOLIO_LAYOUT_VERSION;
use crate::object::hash_blob;
use crate::placement::{Observation, PlacementBase, PlacementKind, PlacementOps};
use crate::placement_fs::PlacementFs;
use crate::registry::{
    self, FolioFormat, NewFolio, NewPlacement, PlacementPath, Registry, RegistryError,
};

/// One path and its optional explicit name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterRequest {
    pub path: PathBuf,
    pub name: Option<FolioName>,
}
impl<P: AsRef<Path>> From<P> for RegisterRequest {
    fn from(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            name: None,
        }
    }
}
impl RegisterRequest {
    pub fn new(path: impl AsRef<Path>, name: FolioName) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            name: Some(name),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NameSource {
    FrontMatter,
    Option,
    FileName { suffix: u32 },
}

/// Exact prefix edit; apply must recompute it against the captured full bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NameWrite {
    None,
    Insert(Edit),
    InsertBlock(Edit),
    Replace { old: String, edit: Edit },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NameHolder {
    Folio(FolioId),
    Path(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefusalReason {
    /// Name the target explicitly instead of its symlink.
    Symlink,
    NotAFile,
    Missing,
    Unreadable,
    InsideHome,
    InsideDirectoryFolio,
    ConflictingNames,
    FileChanged,
    DestinationChanged {
        path: PathBuf,
    },
    /// Renames belong to issue #155.
    NameOnRegisteredPath,
    NameTaken {
        name: FolioName,
        holder: NameHolder,
    },
    InvalidFrontMatterName(FolioNameError),
    FrontMatter(FrontMatterError),
    NameOptionWithFrontMatter,
    NoUsableStem(StemError),
    NameNotWritable {
        reason: String,
    },
}

impl fmt::Display for RefusalReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NameOnRegisteredPath => {
                f.write_str("--name on a registered path is refused; use rename (#155)")
            }
            Self::NameOptionWithFrontMatter => {
                f.write_str("--name is refused when folio-name already exists; use rename (#155)")
            }
            Self::ConflictingNames => f.write_str("one path requested with different names"),
            Self::DestinationChanged { path } => {
                write!(f, "{}: destination changed since the plan", path.display())
            }
            Self::NoUsableStem(error) => error.fmt(f),
            Self::FrontMatter(error) => error.fmt(f),
            Self::InvalidFrontMatterName(error) => write!(f, "invalid folio-name: {error}"),
            Self::NameNotWritable { reason } => write!(f, "cannot write folio-name: {reason}"),
            Self::NameTaken { name, holder } => write!(f, "name {name} is taken by {holder:?}"),
            other => write!(f, "{other:?}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterStatus {
    New,
    AlreadyRegistered,
    Refused(RefusalReason),
    NeedsConfirmation { size: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterEntry {
    pub path: PathBuf,
    pub status: RegisterStatus,
    pub size: u64,
    /// An empty marker to create in the user's directory, with no clobbering.
    pub creates_marker: bool,
    pub name: Option<FolioName>,
    pub name_source: Option<NameSource>,
    pub name_write: NameWrite,
    signature: Option<crate::placement::Signature>,
    prefix: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegisterSummary {
    pub new: usize,
    pub already_registered: usize,
    pub refused: usize,
    pub needs_confirmation: usize,
    pub total_bytes: u64,
    pub files_to_write: usize,
    /// Up to ten entries, largest first (path breaks ties).
    pub largest: Vec<(PathBuf, u64)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterNote {
    NeedsConfirmation { size: u64 },
}

/// The dry-run result, also consumed by apply. Request paths remain private so
/// changing display data cannot bypass apply's checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterPlan {
    pub entries: Vec<RegisterEntry>,
    pub notes: Vec<RegisterNote>,
    pub summary: RegisterSummary,
    requests: Vec<RegisterRequest>,
    // Resolved paths aligned with original requests, before entry deduplication.
    destinations: Vec<PathBuf>,
}

pub fn plan_register(
    home: &Home,
    paths: impl IntoIterator<Item = impl Into<RegisterRequest>>,
) -> Result<RegisterPlan, RegisterError> {
    let settings = Settings::load(home)?;
    let registry = Registry::open_existing(home)?;
    let home_path = fs::canonicalize(home.path())?;
    let mut entries: Vec<RegisterEntry> = Vec::new();
    let mut requests: Vec<RegisterRequest> = Vec::new();
    let mut seen = HashMap::<PathBuf, usize>::new();
    let mut replay = Vec::new();
    let mut destinations = Vec::new();
    for input in paths {
        let mut request = input.into();
        let absolute = if request.path.is_absolute() {
            request.path.clone()
        } else {
            Environment::current_dir()?.join(&request.path)
        };
        request.path = absolute.clone();
        replay.push(request.clone());
        let resolved = PlacementPath::resolve(&absolute);
        let path = resolved
            .as_ref()
            .map_or_else(|_| absolute.clone(), PlacementPath::to_path_buf);
        let directory = fs::symlink_metadata(&absolute).is_ok_and(|m| m.is_dir());
        destinations.push(register_destination(directory, &path, resolved.is_ok()).to_path_buf());
        if let Some(&index) = seen.get(&path) {
            let previous: &mut RegisterRequest = &mut requests[index];
            if previous.name.is_some() && request.name.is_some() && previous.name != request.name {
                entries[index].status = RegisterStatus::Refused(RefusalReason::ConflictingNames);
                continue;
            }
            if previous.name.is_none() {
                previous.name = request.name;
            }
            // Preserve directory intent in either request order.
            if directory {
                previous.path = absolute;
            }
            if !matches!(
                entries[index].status,
                RegisterStatus::Refused(RefusalReason::ConflictingNames)
            ) {
                entries[index] =
                    inspect_entry(&home_path, &registry, &settings, &previous.path, path)?;
            }
            continue;
        }
        seen.insert(path.clone(), entries.len());
        entries.push(inspect_entry(
            &home_path, &registry, &settings, &absolute, path,
        )?);
        requests.push(request);
    }
    resolve_names(&mut entries, &requests, &registry)?;
    Ok(report(
        entries,
        replay,
        destinations,
        settings.batch_total_bytes,
    ))
}

fn register_destination(is_directory: bool, path: &Path, resolution_succeeded: bool) -> &Path {
    if is_directory {
        if resolution_succeeded {
            destination_path(path)
        } else {
            // A failed resolution leaves the request path, not a marker path.
            path
        }
    } else {
        destination_path(path)
    }
}

fn refuse_name(entry: &mut RegisterEntry, reason: RefusalReason) {
    entry.status = RegisterStatus::Refused(reason);
    entry.name = None;
    entry.name_source = None;
    entry.name_write = NameWrite::None;
}

fn markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            MARKDOWN_EXTENSIONS
                .iter()
                .any(|extension| ext.eq_ignore_ascii_case(extension))
        })
}

fn holder(
    registry: &Registry,
    assigned: &HashMap<FolioName, PathBuf>,
    name: &FolioName,
) -> Result<Option<NameHolder>, RegisterError> {
    // Exactly one indexed lookup for each candidate, including local collisions.
    let registered = registry.find_by_name(name)?;
    Ok(registered
        .map(NameHolder::Folio)
        .or_else(|| assigned.get(name).cloned().map(NameHolder::Path)))
}

// split in the folt-core refactor (#203)
#[allow(clippy::too_many_lines)]
fn resolve_names(
    entries: &mut [RegisterEntry],
    requests: &[RegisterRequest],
    registry: &Registry,
) -> Result<(), RegisterError> {
    let mut prefixes = HashMap::new();
    let mut assigned = HashMap::new();
    // Explicit options first, then front matter, preserving request order in each group.
    for (index, (entry, request)) in entries.iter_mut().zip(requests).enumerate() {
        if entry.status == RegisterStatus::AlreadyRegistered && request.name.is_some() {
            refuse_name(entry, RefusalReason::NameOnRegisteredPath);
        }
        if !matches!(
            entry.status,
            RegisterStatus::New | RegisterStatus::NeedsConfirmation { .. }
        ) {
            continue;
        }
        let existing = if markdown(&entry.path) {
            let mut bytes = Vec::new();
            if !entry.creates_marker {
                let read = File::open(&entry.path)
                    .and_then(|file| file.take(SCAN_WINDOW as u64).read_to_end(&mut bytes));
                if let Err(error) = read {
                    refuse_name(entry, io_refusal(&error));
                    continue;
                }
            }
            let name = match front_matter::read_name(&bytes) {
                Ok(name) => name.map(|name| name.into_owned()),
                Err(error) => {
                    refuse_name(entry, RefusalReason::FrontMatter(error));
                    continue;
                }
            };
            entry.prefix = Some(bytes.clone());
            prefixes.insert(index, bytes);
            name
        } else {
            None
        };
        if let Some(value) = existing {
            let name = match FolioName::parse(&value) {
                Ok(name) => name,
                Err(error) => {
                    refuse_name(entry, RefusalReason::InvalidFrontMatterName(error));
                    continue;
                }
            };
            if request.name.is_some() {
                refuse_name(entry, RefusalReason::NameOptionWithFrontMatter);
                continue;
            }
            entry.name = Some(name);
            entry.name_source = Some(NameSource::FrontMatter);
        } else if let Some(name) = &request.name {
            entry.name = Some(name.clone());
            entry.name_source = Some(NameSource::Option);
        }
    }
    for option_pass in [true, false] {
        for entry in entries.iter_mut() {
            if entry.name.is_none()
                || matches!(entry.name_source, Some(NameSource::Option)) != option_pass
            {
                continue;
            }
            let Some(name) = entry.name.clone() else {
                continue;
            };
            if let Some(holder) = holder(registry, &assigned, &name)? {
                refuse_name(entry, RefusalReason::NameTaken { name, holder });
            } else {
                assigned.insert(name, entry.path.clone());
            }
        }
    }
    let mut counters = HashMap::<FolioName, u32>::new();
    for (index, (entry, request)) in entries.iter_mut().zip(requests).enumerate() {
        if !matches!(
            entry.status,
            RegisterStatus::New | RegisterStatus::NeedsConfirmation { .. }
        ) {
            continue;
        }
        if entry.name.is_none() {
            let directory = fs::symlink_metadata(&request.path).is_ok_and(|m| m.is_dir());
            let stem = if directory {
                entry.path.parent().and_then(Path::file_name)
            } else {
                request.path.file_stem()
            };
            let base = match FolioName::from_stem(stem.unwrap_or_default()) {
                Ok(name) => name,
                Err(error) => {
                    refuse_name(entry, RefusalReason::NoUsableStem(error));
                    continue;
                }
            };
            let counter = counters.entry(base.clone()).or_default();
            loop {
                let name = if *counter == 0 {
                    base.clone()
                } else {
                    base.with_suffix(*counter)
                };
                let suffix = *counter;
                *counter = counter
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("name suffix exhausted"))?;
                if holder(registry, &assigned, &name)?.is_none() {
                    assigned.insert(name.clone(), entry.path.clone());
                    entry.name = Some(name);
                    entry.name_source = Some(NameSource::FileName { suffix });
                    break;
                }
            }
        }
        if entry.name_source == Some(NameSource::FrontMatter) {
            continue;
        }
        if let Some(bytes) = prefixes.get(&index) {
            let name = entry.name.as_ref().ok_or_else(|| {
                io::Error::other(format!(
                    "missing resolved folio-name: {}",
                    entry.path.display()
                ))
            })?;
            let edit = match front_matter::set_name(bytes, Some(name.as_str()), true) {
                Ok(Some(edit)) => edit,
                Ok(None) => continue,
                Err(error) => {
                    refuse_name(entry, RefusalReason::FrontMatter(error));
                    continue;
                }
            };
            entry.name_write = if front_matter::locate(bytes)
                .map_err(|e| io::Error::other(e.to_string()))?
                .is_some()
            {
                NameWrite::Insert(edit)
            } else {
                NameWrite::InsertBlock(edit)
            };
            if !entry.creates_marker {
                let meta = fs::metadata(&entry.path)?;
                let reason = if meta.permissions().readonly() {
                    Some("read-only file".to_owned())
                } else {
                    crate::placement_fs::require_single_link(&entry.path)
                        .err()
                        .map(|e| e.to_string())
                };
                if let Some(reason) = reason {
                    refuse_name(entry, RefusalReason::NameNotWritable { reason });
                }
            }
        }
    }
    Ok(())
}

fn inspect_entry(
    home: &Path,
    registry: &Registry,
    settings: &Settings,
    input: &Path,
    path: PathBuf,
) -> Result<RegisterEntry, RegisterError> {
    let mut entry = RegisterEntry {
        path: path.clone(),
        status: RegisterStatus::New,
        size: 0,
        creates_marker: false,
        name: None,
        name_source: None,
        name_write: NameWrite::None,
        signature: None,
        prefix: None,
    };
    let refuse = |mut entry: RegisterEntry, reason| {
        entry.status = RegisterStatus::Refused(reason);
        entry
    };
    if path.starts_with(home) {
        return Ok(refuse(entry, RefusalReason::InsideHome));
    }
    if let Ok(key) = PlacementPath::resolve(input)
        && registry.placement_at(&key)?.is_some()
    {
        entry.status = RegisterStatus::AlreadyRegistered;
        return Ok(entry);
    }
    for ancestor in path.ancestors().skip(1) {
        let marker = ancestor.join(MARKER_FILE);
        if marker != path
            && let Ok(key) = PlacementPath::resolve(&marker)
            && registry.placement_at(&key)?.is_some()
        {
            return Ok(refuse(entry, RefusalReason::InsideDirectoryFolio));
        }
    }
    let named = match fs::symlink_metadata(input) {
        Ok(meta) => meta,
        Err(error) => return Ok(refuse(entry, io_refusal(&error))),
    };
    if named.is_symlink() {
        return Ok(refuse(entry, RefusalReason::Symlink));
    }
    let metadata = if named.is_dir() {
        match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                entry.creates_marker = true;
                return Ok(entry);
            }
            Err(error) => return Ok(refuse(entry, io_refusal(&error))),
        }
    } else {
        named
    };
    if metadata.is_symlink() {
        return Ok(refuse(entry, RefusalReason::Symlink));
    }
    if !metadata.is_file() {
        return Ok(refuse(entry, RefusalReason::NotAFile));
    }
    // Opening a regular file checks access without reading its bytes.
    if File::open(&path).is_err() {
        return Ok(refuse(entry, RefusalReason::Unreadable));
    }
    entry.signature = Some(crate::placement_fs::signature(&metadata));
    entry.size = metadata.len();
    if entry.size >= settings.large_file_bytes {
        entry.status = RegisterStatus::NeedsConfirmation { size: entry.size };
    }
    Ok(entry)
}

fn io_refusal(error: &io::Error) -> RefusalReason {
    if error.kind() == io::ErrorKind::NotFound {
        RefusalReason::Missing
    } else {
        RefusalReason::Unreadable
    }
}

fn report(
    entries: Vec<RegisterEntry>,
    requests: Vec<RegisterRequest>,
    destinations: Vec<PathBuf>,
    batch_total_bytes: u64,
) -> RegisterPlan {
    let mut summary = RegisterSummary::default();
    for entry in &entries {
        if entry.name_write != NameWrite::None {
            summary.files_to_write += 1;
        }
        match entry.status {
            RegisterStatus::New => summary.new += 1,
            RegisterStatus::AlreadyRegistered => summary.already_registered += 1,
            RegisterStatus::Refused(_) => summary.refused += 1,
            RegisterStatus::NeedsConfirmation { .. } => summary.needs_confirmation += 1,
        }
        if matches!(
            entry.status,
            RegisterStatus::New | RegisterStatus::NeedsConfirmation { .. }
        ) {
            summary.total_bytes = summary.total_bytes.saturating_add(entry.size);
            summary.largest.push((entry.path.clone(), entry.size));
        }
    }
    summary
        .largest
        .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    summary.largest.truncate(SUMMARY_LARGEST);
    let notes = if summary.total_bytes > batch_total_bytes {
        vec![RegisterNote::NeedsConfirmation {
            size: summary.total_bytes,
        }]
    } else {
        vec![]
    };
    RegisterPlan {
        entries,
        notes,
        summary,
        requests,
        destinations,
    }
}

impl fmt::Display for RegisterPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for entry in &self.entries {
            if let RegisterStatus::Refused(reason) = &entry.status {
                writeln!(f, "{}: {reason}", entry.path.display())?;
            } else {
                writeln!(f, "{}: {:?}", entry.path.display(), entry.status)?;
            }
            if let Some(name) = &entry.name {
                writeln!(f, "  name: {name}")?;
                if entry.name_write != NameWrite::None {
                    writeln!(
                        f,
                        "will write folio-name: \"{name}\" into {}",
                        entry.path.display()
                    )?;
                }
            }
        }
        writeln!(f, "{} files to write", self.summary.files_to_write)
    }
}

fn is_marker(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(MARKER_FILE))
}

fn destination_path(path: &Path) -> &Path {
    if is_marker(path)
        && let Some(parent) = path.parent()
    {
        // Marker spelling may change during replan; its directory must not.
        parent
    } else {
        path
    }
}

fn destination(entry: &RegisterEntry) -> &Path {
    destination_path(&entry.path)
}

fn block_changed(
    previous: &RegisterPlan,
    mut current: RegisterPlan,
    batch_total_bytes: u64,
) -> RegisterPlan {
    let mut previous_by_destination = HashMap::<_, Vec<_>>::new();
    for entry in &previous.entries {
        previous_by_destination
            .entry(destination(entry))
            .or_default()
            .push(entry);
    }
    for entry in &mut current.entries {
        let request_moved = previous
            .destinations
            .iter()
            .zip(&current.destinations)
            .any(|(old, new)| new == destination(entry) && old != new);
        if request_moved {
            let path = entry.path.clone();
            refuse_name(entry, RefusalReason::DestinationChanged { path });
            continue;
        }
        let old = previous_by_destination
            .get(destination(entry))
            .and_then(|candidates| {
                // Distinct case-variant marker files retain their full path identities.
                // Accept a changed marker spelling only for an unambiguous destination.
                candidates
                    .iter()
                    .copied()
                    .find(|old| old.path == entry.path)
                    .or(match candidates.as_slice() {
                        [old] if is_marker(&old.path) && is_marker(&entry.path) => Some(*old),
                        _ => None,
                    })
            });
        let Some(old) = old else {
            let path = entry.path.clone();
            refuse_name(entry, RefusalReason::DestinationChanged { path });
            continue;
        };
        if matches!(
            entry.status,
            RegisterStatus::New | RegisterStatus::NeedsConfirmation { .. }
        ) && old.prefix.is_some()
            && old.signature.is_some()
            && (old.signature != entry.signature || old.prefix != entry.prefix)
        {
            refuse_name(entry, RefusalReason::FileChanged);
        }
    }
    report(
        current.entries,
        current.requests,
        current.destinations,
        batch_total_bytes,
    )
}

fn blocked(plan: &RegisterPlan, confirm_large: bool) -> bool {
    plan.summary.refused != 0
        || (!confirm_large && (plan.summary.needs_confirmation != 0 || !plan.notes.is_empty()))
}

/// Replans under the exclusive home lock. Returns the plan actually applied,
/// or a blocked plan without writing. Does not create an initial history commit.
pub fn apply_register(
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
) -> Result<RegisterPlan, RegisterError> {
    apply_with_hook(home, plan, confirm_large, |_| Ok(()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterLinkResult {
    Linked,
    LeftAsCopy { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterLinkOutcome {
    pub registration: RegisterPlan,
    pub paths: Vec<(PathBuf, RegisterLinkResult)>,
}

/// Registers the batch before converting each placement independently. The home
/// lock covers probing, registration and conversion; conversion errors keep rows.
pub fn apply_register_links(
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
) -> Result<RegisterLinkOutcome, RegisterError> {
    apply_links_with_hook(home, plan, confirm_large, |step| {
        if let LinkStep::Probe(path) = step {
            PlacementFs::new(home).probe_link(path)?;
        }
        Ok(())
    })
}

#[cfg_attr(not(test), allow(dead_code))]
enum LinkStep<'a> {
    Probe(&'a Path),
    Registered,
    Convert(&'a Path),
}

fn apply_links_with_hook(
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
    mut hook: impl FnMut(LinkStep<'_>) -> Result<(), RegisterError>,
) -> Result<RegisterLinkOutcome, RegisterError> {
    let lock = home.lock()?;
    let settings = Settings::load(home)?;
    let current = block_changed(
        plan,
        plan_register(home, plan.requests.clone())?,
        settings.batch_total_bytes,
    );
    if blocked(&current, confirm_large) {
        return Ok(RegisterLinkOutcome {
            registration: current,
            paths: Vec::new(),
        });
    }
    for entry in &current.entries {
        hook(LinkStep::Probe(&entry.path))?;
    }
    let registration = apply_locked(home, &current, confirm_large, &lock, |_| Ok(()))?;
    if blocked(&registration, confirm_large) {
        return Ok(RegisterLinkOutcome {
            registration,
            paths: Vec::new(),
        });
    }
    hook(LinkStep::Registered)?;
    let paths = registration
        .entries
        .iter()
        .map(|entry| {
            let result = hook(LinkStep::Convert(&entry.path))
                .map_err(|error| error.to_string())
                .and_then(|()| {
                    register_link(home, &entry.path, &lock).map_err(|error| error.to_string())
                });
            let result = match result {
                Ok(()) => RegisterLinkResult::Linked,
                Err(reason) => RegisterLinkResult::LeftAsCopy { reason },
            };
            (entry.path.clone(), result)
        })
        .collect();
    Ok(RegisterLinkOutcome {
        registration,
        paths,
    })
}

fn register_link(home: &Home, path: &Path, lock: &HomeLock) -> Result<(), Box<dyn Error>> {
    use crate::placement_rules::{apply_convert, gather_convert, judge_convert};
    let mut registry = Registry::open_or_create(home, lock)?;
    let row = registry
        .placement_at(&PlacementPath::resolve(path)?)?
        .ok_or_else(|| io::Error::other("placement is missing; run repair (#38)"))?;
    let folio = registry
        .folio_row(row.id)?
        .ok_or_else(|| io::Error::other("Folio is missing"))?;
    let directory = crate::folio_dir::validate(home.path(), &folio)?;
    let objects = crate::store::ObjectStore::open(&directory, &folio.format)?;
    let refs = crate::refs::RefStore::open(&directory, &folio.format)?;
    let fs = PlacementFs::new(home);
    let facts = gather_convert(&row, &fs, |_| Ok(&refs))?;
    if row.kind == PlacementKind::Link && facts.observation == Observation::IntactLink {
        return Ok(());
    }
    let plan = judge_convert(&facts, PlacementKind::Link, None, None)?;
    apply_convert(&plan, &fs, &objects, &mut registry, lock)?;
    Ok(())
}

// Hook payloads are observed by the regression tests.
#[cfg_attr(not(test), allow(dead_code))]
enum Step<'a> {
    Markers,
    Refreshed(&'a mut RegisterPlan),
    MarkerSynced,
    Folio(Durability),
    Transaction,
    Commit,
}

fn apply_with_hook(
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
    hook: impl FnMut(Step<'_>) -> Result<(), RegistryError>,
) -> Result<RegisterPlan, RegisterError> {
    let lock = home.lock()?;
    apply_locked(home, plan, confirm_large, &lock, hook)
}

// split in the folt-core refactor (#203)
#[allow(clippy::too_many_lines)]
fn apply_locked(
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
    lock: &HomeLock,
    mut hook: impl FnMut(Step<'_>) -> Result<(), RegistryError>,
) -> Result<RegisterPlan, RegisterError> {
    let settings = Settings::load(home)?;
    let mut current = block_changed(
        plan,
        plan_register(home, plan.requests.clone())?,
        settings.batch_total_bytes,
    );
    if blocked(&current, confirm_large) {
        return Ok(current);
    }
    let mut markers = Vec::new();
    let mut directories = Vec::new();
    let mut rewritten = Vec::new();
    let result: Result<bool, RegisterError> = (|| {
        hook(Step::Markers)?;
        for entry in &current.entries {
            if entry.creates_marker {
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&entry.path)
                {
                    Ok(file) => {
                        let signature = crate::placement_fs::signature(&file.metadata()?);
                        markers.push((entry.path.clone(), signature));
                        file.sync_all()?;
                        drop(file);
                        if let Some(parent) = entry.path.parent() {
                            crate::fs::sync_directory(parent)?;
                        }
                        hook(Step::MarkerSynced)?;
                    }
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        // An appearing marker must be checked for type, readability and size.
        let mut refreshed = plan_register(home, plan.requests.clone())?;
        hook(Step::Refreshed(&mut refreshed))?;
        current = block_changed(&current, refreshed, settings.batch_total_bytes);
        if blocked(&current, confirm_large) {
            return Ok(false);
        }
        let mut rows = Vec::new();
        // Each index identifies a request in the refreshed plan. Display paths
        // may change spelling during replan, so never use them as identities.
        for (index, entry) in current.entries.clone().into_iter().enumerate() {
            if entry.status == RegisterStatus::AlreadyRegistered {
                continue;
            }
            let mut content = match fs::read(&entry.path) {
                Ok(content) => content,
                Err(error) => {
                    refuse_name(&mut current.entries[index], io_refusal(&error));
                    current = report(
                        current.entries.clone(),
                        current.requests.clone(),
                        current.destinations.clone(),
                        settings.batch_total_bytes,
                    );
                    return Ok(false);
                }
            };
            if let Some(expected) = &entry.signature {
                let metadata = fs::metadata(&entry.path)?;
                if &crate::placement_fs::signature(&metadata) != expected
                    || entry.prefix.as_ref().is_some_and(|prefix| {
                        content.get(..content.len().min(SCAN_WINDOW)) != Some(prefix.as_slice())
                    })
                {
                    refuse_name(&mut current.entries[index], RefusalReason::FileChanged);
                    current = report(
                        current.entries.clone(),
                        current.requests.clone(),
                        current.destinations.clone(),
                        settings.batch_total_bytes,
                    );
                    return Ok(false);
                }
            }
            if entry.name_write != NameWrite::None {
                let edit = front_matter::set_name(
                    &content,
                    entry.name.as_ref().map(FolioName::as_str),
                    true,
                )
                .map_err(|error| io::Error::other(format!("{}: {error}", entry.path.display())))?;
                let planned_edit = match &entry.name_write {
                    NameWrite::Insert(edit)
                    | NameWrite::InsertBlock(edit)
                    | NameWrite::Replace { edit, .. } => edit,
                    NameWrite::None => {
                        return Err(io::Error::other("missing planned folio-name edit").into());
                    }
                };
                if edit.as_ref() != Some(planned_edit) {
                    refuse_name(&mut current.entries[index], RefusalReason::FileChanged);
                    current = report(
                        current.entries.clone(),
                        current.requests.clone(),
                        current.destinations.clone(),
                        settings.batch_total_bytes,
                    );
                    return Ok(false);
                }
                let updated = front_matter::apply_edit(&content, planned_edit)
                    .map_err(|error| io::Error::other(error.to_string()))?;
                let Some(expected) = entry.signature.as_ref() else {
                    refuse_name(&mut current.entries[index], RefusalReason::FileChanged);
                    current = report(
                        current.entries.clone(),
                        current.requests.clone(),
                        current.destinations.clone(),
                        settings.batch_total_bytes,
                    );
                    return Ok(false);
                };
                let write = crate::placement_fs::atomic_rewrite(
                    &entry.path,
                    &updated,
                    expected,
                    |signature| {
                        rewritten.push((entry.path.clone(), content.clone(), signature.clone()));
                        if let Some((_, marker_signature)) =
                            markers.iter_mut().find(|(path, _)| path == &entry.path)
                        {
                            *marker_signature = signature;
                        }
                    },
                );
                if let Err(error) = write {
                    if error.kind() == io::ErrorKind::InvalidData {
                        refuse_name(&mut current.entries[index], RefusalReason::FileChanged);
                        current = report(
                            current.entries.clone(),
                            current.requests.clone(),
                            current.destinations.clone(),
                            settings.batch_total_bytes,
                        );
                        return Ok(false);
                    }
                    return Err(error.into());
                }
                content = updated;
            }
            // A file may grow even after the metadata recheck. Confirmation
            // also covers the bytes actually captured, before publishing them.
            current.entries[index].size = content.len() as u64;
            if content.len() as u64 >= settings.large_file_bytes {
                current.entries[index].status = RegisterStatus::NeedsConfirmation {
                    size: content.len() as u64,
                };
            }
            current = report(
                current.entries.clone(),
                current.requests.clone(),
                current.destinations.clone(),
                settings.batch_total_bytes,
            );
            if blocked(&current, confirm_large) {
                return Ok(false);
            }
            let hash = hash_blob(&content);
            let durability = Durability::DataAndDirectories;
            hook(Step::Folio(durability))?;
            let folio = create_folio(home.path(), lock, FolioSpec { content }, durability)?;
            directories.push(folio.path().to_path_buf());
            let mut internal = registry::internal_placement(home, folio.id())?;
            let mut copy = NewPlacement {
                path: PlacementPath::resolve(&entry.path)?,
                id: folio.id(),
                kind: PlacementKind::Copy,
                head: None,
                commit_id: None,
                base: None,
                staged: None,
            };
            for row in [&mut internal, &mut copy] {
                let placement = crate::placement::Placement {
                    path: row.path.to_path_buf(),
                    kind: row.kind,
                    head: DEFAULT_BRANCH.into(),
                    commit_id: None,
                    base: None,
                };
                let Observation::File { signature } = PlacementFs::new(home)
                    .read(folio.id(), &placement)
                    .map_err(|error| RegisterError::Io(io::Error::other(error)))?
                else {
                    return Err(io::Error::other("registration file changed type").into());
                };
                row.base = Some(PlacementBase { hash, signature });
            }
            rows.push((
                NewFolio {
                    id: folio.id(),
                    format: FolioFormat {
                        layout: FOLIO_LAYOUT_VERSION,
                        algorithm: OBJECT_ALGORITHM.into(),
                    },
                },
                internal,
                copy,
                entry.name.clone().ok_or_else(|| {
                    io::Error::other(format!("missing new folio-name: {}", entry.path.display()))
                })?,
            ));
        }
        if rows.is_empty() {
            return Ok(true);
        }
        // Persist the home entry too when create_folio first created folios/.
        crate::fs::sync_directory(home.path())?;
        hook(Step::Transaction)?;
        let mut registry = Registry::open_or_create(home, lock)?;
        registry.write(lock, |tx| {
            for (folio, internal, copy, name) in &rows {
                tx.add_folio(folio)?;
                tx.set_name(folio.id, name)?;
                tx.add_placement(internal)?;
                tx.add_placement(copy)?;
            }
            hook(Step::Commit)
        })?;
        Ok(true)
    })();
    if matches!(result, Ok(true)) {
        // The empty file used to create an owned marker was only an
        // intermediate state, not an input snapshot for the returned plan.
        for entry in &mut current.entries {
            if markers.iter().any(|(path, _)| path == &entry.path) {
                entry.signature = None;
            }
        }
    }
    if !matches!(result, Ok(true)) {
        let mut failures = Vec::new();
        for (path, original, signature) in rewritten.into_iter().rev() {
            if !fs::symlink_metadata(&path).is_ok_and(|meta| {
                meta.is_file() && crate::placement_fs::signature(&meta) == signature
            }) {
                failures.push(format!(
                    "{} keeps the name: file changed after our write",
                    path.display()
                ));
                continue;
            }
            let mut restoration_published = false;
            if let Err(error) =
                crate::placement_fs::atomic_rewrite(&path, &original, &signature, |restored| {
                    restoration_published = true;
                    if let Some((_, marker_signature)) =
                        markers.iter_mut().find(|(marker, _)| marker == &path)
                    {
                        *marker_signature = restored;
                    }
                })
            {
                if restoration_published {
                    failures.push(format!(
                        "{}: restoration completed but the directory sync failed; original bytes are back, durability of the restore is not confirmed: {error}",
                        path.display()
                    ));
                } else {
                    failures.push(format!(
                        "{} keeps the name: restore failed: {error}",
                        path.display()
                    ));
                }
            }
        }
        for directory in directories {
            if let Err(error) = fs::remove_dir_all(&directory) {
                failures.push(format!("{}: {error}", directory.display()));
            }
        }
        for (marker, signature) in markers {
            if fs::symlink_metadata(&marker).is_ok_and(|meta| {
                meta.is_file() && crate::placement_fs::signature(&meta) == signature
            }) && let Err(error) = fs::remove_file(&marker)
            {
                failures.push(format!("{}: {error}", marker.display()));
            }
        }
        if !failures.is_empty() {
            let cause = match &result {
                Err(error) => error.to_string(),
                _ => "registration blocked".into(),
            };
            return Err(io::Error::other(format!("{cause}; {}", failures.join("; "))).into());
        }
    }
    result.map(|_| current)
}

#[derive(Debug)]
pub enum RegisterError {
    Io(io::Error),
    Config(ConfigError),
    Registry(RegistryError),
    Home(HomeError),
    Folio(FolioDirError),
    Placement(crate::placement::PlacementError),
}
impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Config(e) => e.fmt(f),
            Self::Registry(e) => e.fmt(f),
            Self::Home(e) => e.fmt(f),
            Self::Folio(e) => e.fmt(f),
            Self::Placement(e) => e.fmt(f),
        }
    }
}
impl Error for RegisterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Io(e) => e,
            Self::Config(e) => e,
            Self::Registry(e) => e,
            Self::Home(e) => e,
            Self::Folio(e) => e,
            Self::Placement(e) => e,
        })
    }
}
macro_rules! from_error {
    ($source:ty, $variant:ident) => {
        impl From<$source> for RegisterError {
            fn from(error: $source) -> Self {
                Self::$variant(error)
            }
        }
    };
}
from_error!(io::Error, Io);
from_error!(ConfigError, Config);
from_error!(RegistryError, Registry);
from_error!(HomeError, Home);
from_error!(FolioDirError, Folio);
from_error!(crate::placement::PlacementError, Placement);

#[cfg(test)]
mod tests;
