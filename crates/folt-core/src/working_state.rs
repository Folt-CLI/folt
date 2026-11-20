use crate::config::limits::SCAN_WINDOW;
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::ObjectKind;
use crate::ops::refusal::{Refusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::ops::working::resolve::{read_working, resolve_working};
use crate::placement::{Observation, PlacementError, PlacementKind, PlacementOps, StagedBlob};
use crate::placement_path::RegistryError;
use crate::ref_name::RefKind;
use crate::refs::RefStore;
use crate::registry::{Placement, Registry};
use crate::store::ObjectStore;

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
        return Err(Refusal {
            path: row.path.to_path_buf(),
            reason: RefusalReason::PlacementChanged,
        }
        .into());
    }
    let Observation::File { signature } = before else {
        return Err(Refusal {
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

/// Inspect exactly the prefix staging uses, including internal Markdown detection.
pub(crate) fn stage_references<'a>(
    selected: &Placement,
    prefix: &'a [u8],
) -> Result<Vec<crate::front_matter::Reference<'a>>, crate::front_matter::FrontMatterError> {
    let prefix = crate::front_matter::scan_window(prefix);
    let markdown = crate::front_matter::applies_to(&selected.path.to_path_buf());
    // Internal working files have no Markdown extension. Only their own named
    // front matter enables detection; copies and links use the selected path.
    let markdown = if !markdown && selected.kind == PlacementKind::Internal {
        match crate::front_matter::read_name(prefix) {
            Ok(name) => name.is_some(),
            Err(crate::front_matter::FrontMatterError::Binary) => false,
            Err(error) => return Err(error),
        }
    } else {
        markdown
    };
    if !markdown {
        return Ok(Vec::new());
    }
    match crate::front_matter::read_references(prefix) {
        Ok(refs) => Ok(refs),
        Err(crate::front_matter::FrontMatterError::Binary) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

fn write_stage(
    row: &Placement,
    selected: &Placement,
    staged: Option<StagedBlob>,
    blob: Option<(&ObjectStore, ObjectId, &RefStore)>,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    let markdown = crate::front_matter::applies_to(&selected.path.to_path_buf());
    let prefix = blob
        .filter(|_| markdown || selected.kind == PlacementKind::Internal)
        .map(|(objects, hash, _)| objects.get_prefix(hash, ObjectKind::Blob, SCAN_WINDOW))
        .transpose()
        .map_err(PlacementError::Store)?;
    // Internal working files have no Markdown extension. Only their own named
    // front matter enables detection; copies and links use the selected path.
    if !markdown && selected.kind == PlacementKind::Internal {
        match crate::front_matter::read_name(prefix.as_deref().unwrap_or_default()) {
            Ok(_) | Err(crate::front_matter::FrontMatterError::Binary) => {}
            Err(error) => {
                return Err(RegistryError::References(format!(
                    "{error}; the internal file starts with a front matter block that cannot be read: fix or remove it to stage"
                ))
                .into());
            }
        }
    }
    let references = blob
        .map(|_| stage_references(selected, prefix.as_deref().unwrap_or_default()))
        .transpose()
        .map_err(|error| RegistryError::References(error.to_string()))?;
    let live_branches = blob
        .map(|(_, _, refs)| refs.list(RefKind::Branch))
        .transpose()?;
    registry.write(lock, |tx| {
        if tx.placement_at(&row.path)?.as_ref() != Some(row) {
            return Err(RegistryError::PlacementChanged(row.path.clone()));
        }
        let mut updated = row.clone();
        updated.staged = staged;
        tx.update_placement(&updated)?;
        if let Some(references) = &references {
            let live = live_branches
                .as_ref()
                .ok_or_else(|| {
                    RegistryError::References("missing live branches for staged references".into())
                })?
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>();
            tx.purge_dead_branch_references(row.id, &live)?;
            // Internal rows (including the target of a link) are constrained to
            // main by the placements schema; copies stage on their own head.
            tx.rebuild_references(row.id, &row.head, references)?;
        }
        Ok(())
    })?;
    Ok(())
}
