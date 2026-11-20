use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use crate::config::names::{DEFAULT_BRANCH, FOLIOS_DIR};
use crate::fsx::Signature;
use crate::home::HomeLock;
use crate::id::FolioId;
use crate::id::ObjectId;
use crate::object::{Author, Commit, CommitTime, ObjectError, ObjectKind, Tag, hash_blob};
use crate::ops::facts::PlacementFacts;
use crate::ops::placement::move_::{
    MovePlan, MoveTarget, apply_move, apply_move_file, judge_move, moved_row,
};
use crate::ops::refusal;
use crate::ops::rules_error::RulesError;
use crate::ops::working::judge::judge_working;
use crate::ops::working::resolve::resolve_working;
use crate::ops::working::{Edit, gather};
use crate::placement::{Observation, PlacementBase, PlacementError, PlacementKind, PlacementOps};
use crate::placement_path::RegistryError;
use crate::ref_name::{RefKind, RefName};
use crate::refs::{Expect, RefError, RefStore};
use crate::registry::{Placement, Registry};
use crate::store::{ObjectStore, StoreError};

/// Stores for one Folio and the caller's bound for commit object reads.
#[derive(Clone, Copy)]
pub struct HistoryStores<'a> {
    pub objects: &'a ObjectStore,
    pub refs: &'a RefStore,
    pub object_limit: u64,
}

/// Facts for one commit, including the placements that can identify an advanced tip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitFacts {
    pub working: PlacementFacts,
    pub link: Option<Placement>,
    pub internal: Option<PlacementFacts>,
    pub placements: Vec<Placement>,
    pub tip_commit: Option<Commit>,
}

/// The home lock must cover gathering, judging and applying history operations.
/// `object_limit` bounds commit reads independently of blob size.
pub fn gather_commit(
    row: &Placement,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<CommitFacts, HistoryError> {
    let HistoryStores {
        objects,
        refs,
        object_limit,
    } = *stores;
    let link = (row.kind == PlacementKind::Link).then(|| row.clone());
    let working = gather(row, fs, refs, registry)?;
    let row = &working.placement;
    let placements = registry.placements_of_head(row.id, &row.head)?;
    let internal = if row.kind == PlacementKind::Copy && row.head == DEFAULT_BRANCH {
        let internal = placements
            .iter()
            .find(|row| row.kind == PlacementKind::Internal)
            .ok_or(HistoryRefusal::MissingInternal)?;
        Some(gather_file(internal, fs, refs)?)
    } else {
        None
    };
    let tip_commit = working
        .tip
        .map(|id| objects.get_commit(id, object_limit))
        .transpose()?;
    Ok(CommitFacts {
        working,
        link,
        internal,
        placements,
        tip_commit,
    })
}

/// An allowed commit; content and parent are fixed by the recorded stage and base.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitPlan {
    facts: CommitFacts,
    resume: Option<ObjectId>,
}

impl CommitPlan {
    pub fn placement(&self) -> &Placement {
        &self.facts.working.placement
    }
    pub fn resumed_commit(&self) -> Option<ObjectId> {
        self.resume
    }
}

/// Recognises a published commit whose remaining file/registry work can be finished.
pub fn resume_commit(facts: &CommitFacts) -> Option<ObjectId> {
    let row = &facts.working.placement;
    let staged = row.staged?;
    let tip = facts.working.tip?;
    let commit = facts.tip_commit.as_ref()?;
    (Some(tip) != row.commit_id
        && commit.content == staged.hash
        && commit.parents == row.commit_id.into_iter().collect::<Vec<_>>())
    .then_some(tip)
}

fn matches_requested_commit(actual: &Commit, requested: &Commit) -> bool {
    actual.author == requested.author
        && actual.message == requested.message
        && actual.body.as_deref() == requested.body.as_deref().filter(|body| !body.is_empty())
        && actual.group == requested.group
        && actual.content == requested.content
        && actual.parents == requested.parents
}

pub fn judge_commit(facts: &CommitFacts) -> Result<CommitPlan, HistoryRefusal> {
    let row = &facts.working.placement;
    if row.kind == PlacementKind::Link {
        return Err(HistoryRefusal::Link(row.path.to_path_buf()));
    }
    let staged = row.staged.ok_or(HistoryRefusal::NothingStaged)?;
    if !matches!(facts.working.observation, Observation::File { .. }) {
        return Err(HistoryRefusal::Unusable(row.path.to_path_buf()));
    }
    if row.kind == PlacementKind::Internal && row.head != DEFAULT_BRANCH {
        return Err(HistoryRefusal::InvalidFacts);
    }
    let resume = resume_commit(facts);
    if facts.working.tip != row.commit_id && resume.is_none() {
        return Err(branch_moved(facts));
    }
    if row.kind == PlacementKind::Copy && row.head == DEFAULT_BRANCH {
        let internal = facts
            .internal
            .as_ref()
            .ok_or(HistoryRefusal::MissingInternal)?;
        if internal.placement.id != row.id
            || internal.placement.kind != PlacementKind::Internal
            || internal.placement.head != DEFAULT_BRANCH
        {
            return Err(HistoryRefusal::InvalidFacts);
        }
        // A completed file replacement may precede the atomic registry update.
        let already_written = resume.is_some() && internal.placement_hash == Some(staged.hash);
        if internal.placement.staged.is_some()
            || (judge_working(internal).edit != Edit::Clean && !already_written)
        {
            return Err(HistoryRefusal::InternalEdited(
                internal.placement.path.to_path_buf(),
            ));
        }
        // Only the branch and content affect move permission; apply supplies
        // the new commit id once its caller-provided metadata has been encoded.
        let target = MoveTarget {
            branch: RefName::parse(DEFAULT_BRANCH).map_err(|_| HistoryRefusal::InvalidFacts)?,
            commit: resume.or(row.commit_id).unwrap_or(staged.hash),
            content: staged.hash,
        };
        judge_move(internal, &target, false)?;
    }
    Ok(CommitPlan {
        facts: facts.clone(),
        resume,
    })
}

fn branch_moved(facts: &CommitFacts) -> HistoryRefusal {
    HistoryRefusal::BranchMoved {
        tip: facts.working.tip,
        ahead: facts
            .placements
            .iter()
            .filter(|row| {
                facts.working.tip.is_some()
                    && row.commit_id == facts.working.tip
                    && row.path != facts.working.placement.path
                    && row.kind != PlacementKind::Link
            })
            .map(|row| row.path.to_path_buf())
            .collect(),
    }
}

/// Metadata comes from the caller; core history never reads identity or a clock.
pub struct CommitMetadata {
    pub author: Author,
    pub time: CommitTime,
    pub message: String,
    pub body: Option<String>,
    pub group: Option<String>,
}

/// Publishes objects, then the ref, then files, then both registry rows together.
// split in the folt-core refactor (#203)
#[allow(clippy::too_many_lines)]
pub fn apply_commit(
    plan: &CommitPlan,
    metadata: CommitMetadata,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<ObjectId, HistoryError> {
    let HistoryStores {
        objects,
        refs,
        object_limit,
    } = *stores;
    let row = plan.placement();
    require_row(registry, row)?;
    if let Some(link) = &plan.facts.link {
        require_row(registry, link)?;
        if resolve_working(link, registry, fs)? != *row {
            return Err(HistoryRefusal::RegistryChanged(link.path.to_path_buf()).into());
        }
    }
    require_observation(&plan.facts.working, fs)?;
    let mut facts = gather_commit(row, fs, stores, registry)?;
    let checked = judge_commit(&facts)?;
    if let Some(internal) = &plan.facts.internal {
        require_row(registry, &internal.placement)?;
        if checked.resume.is_none() {
            require_observation(internal, fs)?;
        }
    }
    let staged = row.staged.ok_or(HistoryRefusal::InvalidFacts)?;
    let commit = Commit {
        content: staged.hash,
        parents: row.commit_id.into_iter().collect(),
        author: metadata.author,
        time: metadata.time,
        message: metadata.message,
        body: metadata.body,
        group: metadata.group,
    };
    // Validate all metadata before any writes, even when finishing a prior commit.
    commit.encode_body()?;
    if checked.resume.is_some()
        && !facts
            .tip_commit
            .as_ref()
            .is_some_and(|tip| matches_requested_commit(tip, &commit))
    {
        return Err(branch_moved(&facts).into());
    }
    if staged.hash == hash_blob(b"") {
        if staged.size != 0 {
            return Err(HistoryRefusal::InvalidFacts.into());
        }
        objects.put(lock, ObjectKind::Blob, b"")?;
    } else {
        require_blob(objects, staged.hash)?;
    }
    let mut id = if let Some(id) = checked.resume {
        id
    } else {
        objects.put_commit(lock, &commit)?
    };
    objects.sync_blobs()?;
    let name = RefName::parse(&row.head).map_err(RefError::from)?;
    let expect = row.commit_id.map_or(Expect::Absent, Expect::Is);
    match refs.update(lock, RefKind::Branch, &name, id, expect) {
        Ok(()) => {}
        Err(RefError::Stale { actual }) => {
            facts.working.tip = actual;
            facts.tip_commit = actual
                .map(|id| objects.get_commit(id, object_limit))
                .transpose()?;
            if !facts
                .tip_commit
                .as_ref()
                .is_some_and(|tip| matches_requested_commit(tip, &commit))
            {
                return Err(branch_moved(&facts).into());
            }
            id = judge_commit(&facts)?
                .resume
                .ok_or_else(|| branch_moved(&facts))?;
        }
        Err(error) => return Err(error.into()),
    }
    refs.sync_refs()?;
    let internal = if let Some(internal) = &facts.internal {
        let target = MoveTarget {
            branch: name,
            commit: id,
            content: staged.hash,
        };
        let move_plan = judge_move(internal, &target, false)?;
        let base = apply_move_file(&move_plan, fs, objects, registry, lock)?;
        Some((internal.placement.clone(), moved_row(&move_plan, base)))
    } else {
        None
    };
    let mut updated = row.clone();
    updated.commit_id = Some(id);
    updated.staged = None;
    let Observation::File { signature } = &facts.working.observation else {
        return Err(HistoryRefusal::InvalidFacts.into());
    };
    updated.base = Some(PlacementBase {
        hash: staged.hash,
        signature: if facts.working.placement_hash == Some(staged.hash) {
            signature.clone()
        } else {
            // Post-stage edits stay in the file; their metadata is not the staged base.
            Signature {
                size: staged.size,
                mtime: None,
                ctime: None,
                inode: None,
            }
        },
    });
    registry.write(lock, |tx| {
        if tx.placement_at(&row.path)?.as_ref() != Some(row) {
            return Err(RegistryError::PlacementChanged(row.path.clone()));
        }
        if let Some((before, after)) = &internal {
            if tx.placement_at(&before.path)?.as_ref() != Some(before) {
                return Err(RegistryError::PlacementChanged(before.path.clone()));
            }
            tx.update_placement(after)?;
        }
        tx.update_placement(&updated)
    })?;
    Ok(id)
}

fn require_row(registry: &Registry, row: &Placement) -> Result<(), HistoryError> {
    if registry.placement_at(&row.path)?.as_ref() != Some(row) {
        return Err(HistoryRefusal::RegistryChanged(row.path.to_path_buf()).into());
    }
    Ok(())
}

fn require_observation(facts: &PlacementFacts, fs: &impl PlacementOps) -> Result<(), HistoryError> {
    if fs.read(facts.placement.id, &facts.placement.to_placement().1)? != facts.observation {
        return Err(HistoryRefusal::PlacementChanged(facts.placement.path.to_path_buf()).into());
    }
    Ok(())
}

pub fn create_branch(
    id: FolioId,
    name: &RefName,
    commit: ObjectId,
    refs: &RefStore,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<(), HistoryError> {
    if let Some(actual) = refs.read(RefKind::Branch, name)? {
        return Err(crate::refs::RefError::Stale {
            actual: Some(actual),
        }
        .into());
    }
    // Match ref-update target validation before changing any cached rows.
    let row = registry
        .folio_row(id)?
        .ok_or(RegistryError::FolioMissing(id))?;
    let directory =
        crate::folio_dir::validate(registry.home_path(), &row).map_err(|error| StoreError::Io {
            path: registry.home_path().join(FOLIOS_DIR).join(id.to_string()),
            source: std::io::Error::other(error),
        })?;
    let objects = ObjectStore::open(&directory, &row.format)?;
    let actual = objects.kind_of(commit).map_err(|error| match error {
        StoreError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            RefError::MissingTarget(commit)
        }
        error => RefError::Store(error),
    })?;
    if actual != ObjectKind::Commit {
        return Err(RefError::WrongKind { id: commit, actual }.into());
    }
    // Placements keep a branch live even before its first ref exists. Only a
    // dead name must lose its previous incarnation's cache before publication.
    if registry.placements_of_head(id, name.as_str())?.is_empty() {
        registry.write(lock, |tx| tx.delete_branch_references(id, name.as_str()))?;
    }
    refs.update(lock, RefKind::Branch, name, commit, Expect::Absent)?;
    refs.sync_refs()?;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResetFacts {
    pub id: FolioId,
    pub target: MoveTarget,
    pub old: ObjectId,
    pub placements: Vec<PlacementFacts>,
}

pub fn gather_reset(
    id: FolioId,
    branch: &RefName,
    commit: ObjectId,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<ResetFacts, HistoryError> {
    let HistoryStores {
        objects,
        refs,
        object_limit,
    } = *stores;
    let old = refs
        .read(RefKind::Branch, branch)?
        .ok_or(HistoryRefusal::MissingBranch)?;
    let target = objects.get_commit(commit, object_limit)?;
    require_blob(objects, target.content)?;
    let rows = registry.placements_of_head(id, branch.as_str())?;
    let mut placements = Vec::new();
    for row in rows {
        // Links follow the internal file and have no recorded commit or base to move.
        if row.kind == PlacementKind::Link {
            continue;
        }
        placements.push(gather_file(&row, fs, refs)?);
    }
    Ok(ResetFacts {
        id,
        target: MoveTarget {
            branch: branch.clone(),
            commit,
            content: target.content,
        },
        old,
        placements,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResetPlan {
    facts: ResetFacts,
    moves: Vec<MovePlan>,
}

pub fn judge_reset(facts: &ResetFacts, discard: bool) -> Result<ResetPlan, HistoryRefusal> {
    let mut moves = Vec::new();
    for placement in &facts.placements {
        if placement.placement.id != facts.id
            || placement.placement.head != facts.target.branch.as_str()
            || placement.tip != Some(facts.old)
        {
            return Err(HistoryRefusal::InvalidFacts);
        }
        moves.push(judge_move(placement, &facts.target, discard)?);
    }
    Ok(ResetPlan {
        facts: facts.clone(),
        moves,
    })
}

pub fn apply_reset(
    plan: &ResetPlan,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<(), HistoryError> {
    let HistoryStores { objects, refs, .. } = *stores;
    let facts = &plan.facts;
    let actual = registry.placements_of_head(facts.id, facts.target.branch.as_str())?;
    let expected: Vec<_> = facts
        .placements
        .iter()
        .map(|facts| facts.placement.clone())
        .collect();
    let actual: Vec<_> = actual
        .into_iter()
        .filter(|row| row.kind != PlacementKind::Link)
        .collect();
    if actual != expected {
        return Err(HistoryRefusal::PlacementsChanged.into());
    }
    for placement in &facts.placements {
        require_observation(placement, fs)?;
    }
    require_blob(objects, facts.target.content)?;
    objects.sync_blobs()?;
    refs.update(
        lock,
        RefKind::Branch,
        &facts.target.branch,
        facts.target.commit,
        Expect::Is(facts.old),
    )?;
    refs.sync_refs()?;
    for movement in &plan.moves {
        apply_move(movement, fs, objects, registry, lock)?;
    }
    Ok(())
}

/// Pass `placements_of_head`, including the internal row, to this pure judge.
pub fn judge_delete_branch(placements: &[Placement]) -> Result<(), HistoryRefusal> {
    if placements.is_empty() {
        Ok(())
    } else {
        Err(HistoryRefusal::BranchInUse(
            placements
                .iter()
                .map(|row| row.path.to_path_buf())
                .collect(),
        ))
    }
}

pub fn delete_branch(
    id: FolioId,
    name: &RefName,
    old: ObjectId,
    refs: &RefStore,
    registry: &Registry,
    lock: &mut HomeLock,
) -> Result<(), HistoryError> {
    judge_delete_branch(&registry.placements_of_head(id, name.as_str())?)?;
    refs.delete(lock, RefKind::Branch, name, Expect::Is(old))?;
    refs.sync_refs()?;
    Ok(())
}

/// `None` makes a lightweight tag; an annotation must name this ref and target.
pub fn create_tag(
    name: &RefName,
    commit: ObjectId,
    annotation: Option<&Tag>,
    stores: &HistoryStores<'_>,
    lock: &mut HomeLock,
) -> Result<ObjectId, HistoryError> {
    let HistoryStores { objects, refs, .. } = *stores;
    if let Some(tag) = annotation {
        if tag.name != name.as_str() || tag.target != commit {
            return Err(HistoryRefusal::TagMismatch.into());
        }
        tag.encode_body()?;
    }
    if refs.read(RefKind::Tag, name)?.is_some() {
        return Err(HistoryRefusal::TagExists.into());
    }
    let actual = objects.kind_of(commit)?;
    if actual != ObjectKind::Commit {
        return Err(RefError::WrongKind { id: commit, actual }.into());
    }
    let target = match annotation {
        Some(tag) => objects.put_tag(lock, tag)?,
        None => commit,
    };
    objects.sync_blobs()?;
    refs.update(lock, RefKind::Tag, name, target, Expect::Absent)?;
    refs.sync_refs()?;
    Ok(target)
}

pub fn delete_tag(
    name: &RefName,
    old: ObjectId,
    refs: &RefStore,
    lock: &mut HomeLock,
) -> Result<(), HistoryError> {
    refs.delete(lock, RefKind::Tag, name, Expect::Is(old))?;
    refs.sync_refs()?;
    Ok(())
}

fn gather_file(
    row: &Placement,
    fs: &impl PlacementOps,
    refs: &RefStore,
) -> Result<PlacementFacts, HistoryError> {
    let (_, placement) = row.to_placement();
    let observation = fs.read(row.id, &placement)?;
    let placement_hash = if matches!(observation, Observation::File { .. }) {
        Some(fs.hash_placement(&placement)?)
    } else {
        None
    };
    Ok(PlacementFacts {
        placement: row.clone(),
        observation,
        placement_hash,
        tip: refs.read(
            RefKind::Branch,
            &RefName::parse(&row.head).map_err(RefError::from)?,
        )?,
    })
}

fn require_blob(objects: &ObjectStore, id: ObjectId) -> Result<(), HistoryError> {
    let actual = objects.kind_of(id)?;
    if actual != ObjectKind::Blob {
        return Err(RefError::WrongKind { id, actual }.into());
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryRefusal {
    NothingStaged,
    BranchMoved {
        tip: Option<ObjectId>,
        ahead: Vec<PathBuf>,
    },
    InternalEdited(PathBuf),
    MissingInternal,
    MissingBranch,
    Link(PathBuf),
    Unusable(PathBuf),
    RegistryChanged(PathBuf),
    PlacementChanged(PathBuf),
    PlacementsChanged,
    BranchInUse(Vec<PathBuf>),
    InvalidFacts,
    TagMismatch,
    TagExists,
    Move(refusal::PlacementRefusal),
}

impl From<refusal::PlacementRefusal> for HistoryRefusal {
    fn from(error: refusal::PlacementRefusal) -> Self {
        Self::Move(error)
    }
}

impl fmt::Display for HistoryRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NothingStaged => write!(f, "nothing staged"),
            Self::BranchMoved { tip, ahead } => {
                write!(f, "branch moved to {tip:?}")?;
                for path in ahead {
                    write!(f, "; ahead at {}", path.display())?;
                }
                Ok(())
            }
            Self::InternalEdited(path) => write!(
                f,
                "folio.main holds edits or a stage: {}; commit through a link or the internal row, or checkout main with discard",
                path.display()
            ),
            Self::MissingInternal => write!(f, "missing internal working-file row"),
            Self::MissingBranch => write!(f, "branch has no tip"),
            Self::Link(path) => write!(f, "cannot commit a link: {}", path.display()),
            Self::Unusable(path) => write!(f, "working file is unusable: {}", path.display()),
            Self::RegistryChanged(path) => write!(f, "registry row changed: {}", path.display()),
            Self::PlacementChanged(path) => write!(f, "working file changed: {}", path.display()),
            Self::PlacementsChanged => {
                write!(f, "branch placements changed since reset was judged")
            }
            Self::BranchInUse(paths) => write!(f, "branch is used by {} placement(s)", paths.len()),
            Self::InvalidFacts => write!(f, "inconsistent history facts"),
            Self::TagMismatch => write!(f, "annotation name or target differs from the tag ref"),
            Self::TagExists => write!(f, "tag already exists"),
            Self::Move(error) => error.fmt(f),
        }
    }
}
impl Error for HistoryRefusal {}

#[derive(Debug)]
pub enum HistoryError {
    Refused(HistoryRefusal),
    Object(ObjectError),
    Store(StoreError),
    Ref(RefError),
    Registry(RegistryError),
    Placement(PlacementError),
    Rules(RulesError),
}

impl From<HistoryRefusal> for HistoryError {
    fn from(error: HistoryRefusal) -> Self {
        Self::Refused(error)
    }
}
impl From<ObjectError> for HistoryError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}
impl From<StoreError> for HistoryError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}
impl From<RefError> for HistoryError {
    fn from(error: RefError) -> Self {
        Self::Ref(error)
    }
}
impl From<RegistryError> for HistoryError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}
impl From<PlacementError> for HistoryError {
    fn from(error: PlacementError) -> Self {
        Self::Placement(error)
    }
}
impl From<RulesError> for HistoryError {
    fn from(error: RulesError) -> Self {
        Self::Rules(error)
    }
}
impl From<refusal::PlacementRefusal> for HistoryError {
    fn from(error: refusal::PlacementRefusal) -> Self {
        Self::Refused(error.into())
    }
}

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(error) => error.fmt(f),
            Self::Object(error) => error.fmt(f),
            Self::Store(error) => error.fmt(f),
            Self::Ref(error) => error.fmt(f),
            Self::Registry(error) => error.fmt(f),
            Self::Placement(error) => error.fmt(f),
            Self::Rules(error) => error.fmt(f),
        }
    }
}
impl Error for HistoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Refused(error) => Some(error),
            Self::Object(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Ref(error) => Some(error),
            Self::Registry(error) => Some(error),
            Self::Placement(error) => Some(error),
            Self::Rules(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests;
