use super::model::PruneFacts;
use crate::config::limits::PRUNE_CONFIRM_MIN;
use crate::ops::stage::refuse::is_conflicted;
use crate::placement::{self, Drift, Observation, PlacementKind};

/// Returns None only for a missing external row with an existing parent.
/// All decisions depend solely on supplied facts.
pub(crate) fn judge(facts: &PruneFacts) -> Option<String> {
    if facts.row.kind == PlacementKind::Internal {
        return Some(match &facts.observation {
            Err(error) => {
                format!("internal placement; read error: {error}; rebuild belongs to #51")
            }
            Ok(Observation::Missing) => "internal placement missing; damaged home (#51)".into(),
            Ok(_) => "internal placement".into(),
        });
    }
    if is_conflicted(&facts.row) {
        return Some(
            "copy has a conflict state; resolve and stage, or remove the placement".into(),
        );
    }
    let observation = match &facts.observation {
        Ok(observation) => observation,
        Err(error) => return Some(format!("read error: {error}")),
    };
    match &facts.parent_exists {
        Ok(true) => {}
        Ok(false) => return Some("parent directory missing".into()),
        Err(error) => return Some(format!("parent read error: {error}")),
    }
    if observation == &Observation::Missing {
        return None;
    }
    let (_, placement) = facts.row.to_placement();
    if let (Observation::File { signature }, Some(stage)) = (observation, facts.row.staged)
        && facts.hash == Some(stage.hash)
        && signature.size == stage.size
    {
        return Some("matches stage".into());
    }
    Some(
        match placement::classify(&placement, observation, facts.hash) {
            Drift::InSync => "in sync",
            Drift::Detached => "a different file than expected",
            Drift::PlacementEdited => "edited or different",
            Drift::Unusable => "unusable entry",
            Drift::Gone => return None,
        }
        .into(),
    )
}

/// Requires at least five drops AND strictly more than half the scoped rows.
/// Division avoids overflow when comparing counts.
pub(crate) fn needs_confirmation(dropped: usize, in_scope: usize) -> bool {
    dropped >= PRUNE_CONFIRM_MIN && dropped > in_scope / 2
}
