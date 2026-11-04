use crate::config::limits::SCAN_WINDOW;
use crate::config::names::DEFAULT_BRANCH;
use crate::config::names::MARKDOWN_EXTENSIONS;
use crate::home::HomeLock;
use crate::object::{ObjectId, ObjectKind};
use crate::placement::{Observation, PlacementError, PlacementKind, PlacementOps, StagedBlob};
use crate::placement_rules::{
    Behind, PlacementFacts, Refusal, RefusalReason, RulesError, needs_hash,
};
use crate::refs::{Kind, RefName, RefStore};
use crate::registry::{Placement, Registry, RegistryError};
use crate::store::ObjectStore;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Edit {
    Clean,
    Edited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Staged {
    None,
    Staged,
    StagedThenModified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingStatus {
    pub edit: Edit,
    pub staged: Staged,
    pub behind: Behind,
}

/// Collects one working file, hashing when it has no base or a recorded size matches.
pub fn gather(
    row: &Placement,
    fs: &impl PlacementOps,
    refs: &RefStore,
    registry: &Registry,
) -> Result<PlacementFacts, RulesError> {
    let row = &resolve_working(row, registry, fs)?;
    let observation = read_working(row, fs)?;
    let placement_hash = if needs_hash(row, &observation) {
        Some(fs.hash_placement(&row.to_placement().1)?)
    } else {
        None
    };
    let tip = refs.read(Kind::Branch, &RefName::parse(&row.head)?)?;
    Ok(PlacementFacts {
        placement: row.clone(),
        observation,
        placement_hash,
        tip,
    })
}

/// Compares only the supplied observations; no commits or files are read.
pub fn judge_working(facts: &PlacementFacts) -> WorkingStatus {
    let row = &facts.placement;
    let matches = |hash: ObjectId, size: u64| {
        matches!(&facts.observation, Observation::File { signature } if signature.size == size)
            && facts.placement_hash == Some(hash)
    };
    let edit = if row
        .base
        .as_ref()
        .is_some_and(|base| matches(base.hash, base.signature.size))
    {
        Edit::Clean
    } else {
        Edit::Edited
    };
    let staged = match row.staged {
        None => Staged::None,
        Some(staged) if matches(staged.hash, staged.size) => Staged::Staged,
        Some(_) => Staged::StagedThenModified,
    };
    let behind = match (row.commit_id, facts.tip) {
        (None, None) => Behind::NoHistory,
        (None, Some(_)) => Behind::Unrecorded,
        (Some(_), None) => Behind::MissingTip,
        (Some(commit), Some(tip)) if commit == tip => Behind::UpToDate,
        (Some(_), Some(tip)) => Behind::Behind { tip },
    };
    WorkingStatus {
        edit,
        staged,
        behind,
    }
}

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
    let prefix = &prefix[..prefix.len().min(SCAN_WINDOW)];
    let markdown = selected
        .path
        .to_path_buf()
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            MARKDOWN_EXTENSIONS
                .iter()
                .any(|extension| ext.eq_ignore_ascii_case(extension))
        });
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
    let markdown = selected
        .path
        .to_path_buf()
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            MARKDOWN_EXTENSIONS
                .iter()
                .any(|extension| ext.eq_ignore_ascii_case(extension))
        });
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
        .map(|(_, _, refs)| refs.list(Kind::Branch))
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

/// Links share the internal working row, provided their entry still points to it.
pub fn resolve_working(
    row: &Placement,
    registry: &Registry,
    fs: &impl PlacementOps,
) -> Result<Placement, RulesError> {
    if row.kind != PlacementKind::Link {
        return Ok(row.clone());
    }
    let observation = fs.read(row.id, &row.to_placement().1)?;
    let internal = registry.internal_placement(row.id)?;
    resolve_observed(row, &observation, internal)
}

fn resolve_observed(
    row: &Placement,
    observation: &Observation,
    internal: Option<Placement>,
) -> Result<Placement, RulesError> {
    if row.kind != PlacementKind::Link {
        return Ok(row.clone());
    }
    let reason = match observation {
        Observation::IntactLink => {
            return internal
                .filter(|internal| {
                    internal.id == row.id
                        && internal.kind == PlacementKind::Internal
                        && internal.head == DEFAULT_BRANCH
                })
                .ok_or_else(|| {
                    Refusal {
                        path: row.path.to_path_buf(),
                        reason: RefusalReason::MissingInternal,
                    }
                    .into()
                });
        }
        Observation::Missing => RefusalReason::Gone,
        Observation::Directory | Observation::NotAFile => RefusalReason::NotAFile,
        Observation::File { .. } | Observation::OtherLink { .. } => RefusalReason::LinkReplaced,
    };
    Err(Refusal {
        path: row.path.to_path_buf(),
        reason,
    }
    .into())
}

fn require_working(row: &Placement) -> Result<(), RulesError> {
    if row.kind == PlacementKind::Link {
        return Err(Refusal {
            path: row.path.to_path_buf(),
            reason: RefusalReason::Link,
        }
        .into());
    }
    Ok(())
}

fn read_working(row: &Placement, fs: &impl PlacementOps) -> Result<Observation, RulesError> {
    require_working(row)?;
    let observation = fs.read(row.id, &row.to_placement().1)?;
    let reason = match observation {
        Observation::File { .. } => return Ok(observation),
        Observation::Missing => RefusalReason::Gone,
        Observation::Directory | Observation::NotAFile => RefusalReason::NotAFile,
        Observation::IntactLink | Observation::OtherLink { .. } => RefusalReason::Unusable,
    };
    Err(Refusal {
        path: row.path.to_path_buf(),
        reason,
    }
    .into())
}

#[cfg(test)]
mod tests;
