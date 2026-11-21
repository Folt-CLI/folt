use std::io;

use crate::config::formats::OBJECT_ALGORITHM;
use crate::config::names::DEFAULT_BRANCH;
use crate::folio::dir::{Durability, FolioSpec, create_folio_with};
use crate::folio::format::FolioFormat;
use crate::folio::name::FolioName;
use crate::folio::row::NewFolio;
use crate::fsx::FileSystem;
use crate::home::{Home, HomeLock};
use crate::layout::FOLIO_LAYOUT_VERSION;
use crate::object::hash_blob;
use crate::ops::rollback::Rollback;
use crate::ops::step_hook::StepHook;
use crate::placement::{Observation, PlacementBase, PlacementKind, PlacementOps};
use crate::placement_fs::PlacementFs;
use crate::placement_path::{PlacementPath, RegistryError};
use crate::registry::{self, NewPlacement, Registry};

use super::super::error::RegisterError;
use super::super::plan::RegisterEntry;
use super::Step;

type Rows = (NewFolio, NewPlacement, NewPlacement, FolioName);

pub(super) fn publish_entry(
    fs: &dyn FileSystem,
    home: &Home,
    lock: &HomeLock,
    entry: &RegisterEntry,
    content: Vec<u8>,
    state: &mut Rollback<'_>,
    hook: &mut impl for<'a> StepHook<Step<'a>, RegistryError>,
) -> Result<Rows, RegisterError> {
    let hash = hash_blob(&content);
    let durability = Durability::DataAndDirectories;
    hook.on_step(Step::Folio(durability))?;
    let folio = create_folio_with(fs, home.path(), lock, FolioSpec { content }, durability)?;
    state.record_directory(folio.path().to_path_buf());
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
        let placement = crate::placement::PlacementSpec {
            path: row.path.to_path_buf(),
            kind: row.kind,
            head: DEFAULT_BRANCH.into(),
            commit_id: None,
            base: None,
        };
        let Observation::File { signature } = PlacementFs::with_fs(home, fs)
            .read(folio.id(), &placement)
            .map_err(|error| RegisterError::Io(io::Error::other(error)))?
        else {
            return Err(io::Error::other("registration file changed type").into());
        };
        row.base = Some(PlacementBase { hash, signature });
    }
    Ok((
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
    ))
}

pub(super) fn publish_rows(
    fs: &dyn FileSystem,
    home: &Home,
    lock: &HomeLock,
    rows: &[Rows],
    hook: &mut impl for<'a> StepHook<Step<'a>, RegistryError>,
) -> Result<(), RegisterError> {
    if rows.is_empty() {
        return Ok(());
    }
    // Persist the home entry too when create_folio first created folios/.
    fs.sync_directory(home.path())?;
    hook.on_step(Step::Transaction)?;
    let mut registry = Registry::open_or_create(home, lock)?;
    registry.write(lock, |tx| {
        for (folio, internal, copy, name) in rows {
            tx.add_folio(folio)?;
            tx.set_name(folio.id, name)?;
            tx.add_placement(internal)?;
            tx.add_placement(copy)?;
        }
        hook.on_step(Step::Commit)
    })?;
    Ok(())
}
