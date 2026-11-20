use crate::home::HomeLock;
use crate::object::ObjectKind;
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::ops::working::{read_working, resolve_working};
use crate::placement::{Observation, PlacementError, PlacementOps, StagedBlob};
use crate::refs::RefStore;
use crate::registry::{Placement, Registry};
use crate::store::ObjectStore;
use write::write_stage;

/// Captures a blob, then records its stage without changing the working base.
/// The home lock must cover selecting the row and staging it.
pub fn stage(
    row: &Placement,
    fs: &impl PlacementOps,
    objects: &ObjectStore,
    refs: &RefStore,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    let selected = row;
    let row = &resolve_working(selected, registry, fs)?;
    let before = read_working(row, fs)?;
    let hash = objects
        .put_file(lock, ObjectKind::Blob, &row.path.to_path_buf())
        .map_err(PlacementError::Store)?;
    if read_working(row, fs)? != before {
        return Err(PlacementRefusal {
            path: row.path.to_path_buf(),
            reason: RefusalReason::PlacementChanged,
        }
        .into());
    }
    let Observation::File { signature } = before else {
        return Err(PlacementRefusal {
            path: row.path.to_path_buf(),
            reason: RefusalReason::NotAFile,
        }
        .into());
    };
    let staged = if row.base.as_ref().is_some_and(|base| base.hash == hash) {
        None
    } else {
        Some(StagedBlob {
            hash,
            size: signature.size,
        })
    };
    resolve_working(selected, registry, fs)?;
    write_stage(
        row,
        selected,
        staged,
        Some((objects, hash, refs)),
        registry,
        lock,
    )
}

/// Clears the recorded stage without reading or changing the working file.
pub fn unstage(
    row: &Placement,
    fs: &impl PlacementOps,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    let row = &resolve_working(row, registry, fs)?;
    write_stage(row, row, None, None, registry, lock)
}

mod references;
pub(crate) mod refuse;
mod write;

pub(crate) use references::stage_references;
