use std::borrow::Borrow;
use std::collections::HashMap;

use crate::history::{RefError, RefStore};
use crate::id::{FolioId, ObjectId};
use crate::ops::rules_error::RulesError;
use crate::placement::{self, Observation, PlacementKind, PlacementOps};
use crate::ref_name::{RefKind, RefName};
use crate::registry;

/// Read-only facts used by status and move decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementFacts {
    pub placement: registry::Placement,
    pub observation: Observation,
    pub placement_hash: Option<ObjectId>,
    pub tip: Option<ObjectId>,
}

pub(crate) fn needs_hash(row: &registry::Placement, observation: &Observation) -> bool {
    placement::needs_hash(&row.to_placement().1, observation)
        || matches!((observation, row.staged),
            (Observation::File { signature }, Some(staged)) if signature.size == staged.size)
}

/// Tip comparison only; this does not inspect commit ancestry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Behind {
    UpToDate,
    Behind {
        tip: ObjectId,
    },
    NoHistory,
    Unrecorded,
    /// A recorded commit remains, but its branch ref is missing.
    MissingTip,
}

/// Collects only the supplied rows, reading each distinct Folio/branch tip once.
/// The caller selects rows with `placement_at` for one path or its own set query.
/// `open_refs` supplies a ref store for the requested Folio without selecting placements.
pub fn gather<R: Borrow<RefStore>>(
    placements: &[registry::Placement],
    fs: &impl PlacementOps,
    mut open_refs: impl FnMut(FolioId) -> Result<R, RefError>,
) -> Result<Vec<PlacementFacts>, RulesError> {
    let mut stores = HashMap::new();
    let mut tips = HashMap::new();
    let mut facts = Vec::with_capacity(placements.len());
    for row in placements {
        let (id, placement) = row.to_placement();
        let observation = fs.read(id, &placement)?;
        let placement_hash = if needs_hash(row, &observation) {
            Some(fs.hash_placement(&placement)?)
        } else {
            None
        };
        let key = (id, row.head.clone());
        let tip = if let Some(tip) = tips.get(&key) {
            *tip
        } else {
            let name = RefName::parse(&row.head).map_err(RefError::from)?;
            let refs = match stores.entry(id) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => entry.insert(open_refs(id)?),
            };
            let refs: &RefStore = (*refs).borrow();
            let tip = refs.read(RefKind::Branch, &name)?;
            tips.insert(key, tip);
            tip
        };
        facts.push(PlacementFacts {
            placement: row.clone(),
            observation,
            placement_hash,
            tip,
        });
    }
    Ok(facts)
}

/// Collects one move's facts, also hashing size-changed copies to compare with
/// incoming content. Status gathering never needs that extra comparison.
pub fn gather_move<R: Borrow<RefStore>>(
    placement: &registry::Placement,
    fs: &impl PlacementOps,
    open_refs: impl FnMut(FolioId) -> Result<R, RefError>,
) -> Result<PlacementFacts, RulesError> {
    let mut facts = gather(std::slice::from_ref(placement), fs, open_refs)?;
    let mut facts = facts
        .pop()
        .ok_or(RulesError::Internal("missing gathered placement"))?;
    if placement.kind != PlacementKind::Link
        && matches!(facts.observation, Observation::File { .. })
        && facts.placement_hash.is_none()
    {
        facts.placement_hash = Some(fs.hash_placement(&placement.to_placement().1)?);
    }
    Ok(facts)
}

/// Collects conversion facts, hashing regular files even when the row is a link
/// so a conversion interrupted before its row write can be resumed.
pub fn gather_convert<R: Borrow<RefStore>>(
    placement: &registry::Placement,
    fs: &impl PlacementOps,
    open_refs: impl FnMut(FolioId) -> Result<R, RefError>,
) -> Result<PlacementFacts, RulesError> {
    let mut facts = gather_move(placement, fs, open_refs)?;
    if matches!(facts.observation, Observation::File { .. }) && facts.placement_hash.is_none() {
        facts.placement_hash = Some(fs.hash_placement(&placement.to_placement().1)?);
    }
    Ok(facts)
}
