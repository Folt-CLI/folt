use super::{
    RebaseFacts, RebaseOutcome, replay,
    stopped::{self, RebaseStep},
};
use crate::home::HomeLock;
use crate::object::{ObjectKind, hash_blob};
use crate::ops::history::{
    HistoryError, HistoryRefusal, HistoryStores, gather_file, gather_reset, require_observation,
};
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::stage::refuse::markers_remain;
use crate::ops::step_hook::StepHook;
use crate::placement::PlacementOps;
use crate::ref_name::{RefKind, RefName};
use crate::registry::{Placement, Registry, StoppedRebase};

pub(super) fn continue_steps(
    row: &Placement,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
    mut hook: impl StepHook<RebaseStep, HistoryError>,
) -> Result<RebaseOutcome, HistoryError> {
    let mut progress = stopped::checked(row, registry)?;
    require_tip(&progress, stores)?;
    if progress.current_replay.is_none() {
        return super::finish::finish(row, &progress, fs, stores, registry, lock, hook);
    }
    let state = row.conflict.as_ref().ok_or(HistoryRefusal::InvalidFacts)?;
    let observation = gather_file(row, fs, stores.refs)?;
    let working = stopped::read_file(row)?;
    if observation.placement_hash != Some(hash_blob(&working)) {
        return Err(HistoryRefusal::PlacementChanged(row.path.to_path_buf()).into());
    }
    // The row was durable before the conflict file. Recover that publication first.
    if hash_blob(&working) == progress.file_before
        && Some(progress.file_before) != progress.marker_blob
    {
        let marker = progress.marker_blob.ok_or(HistoryRefusal::InvalidFacts)?;
        fs.refresh(
            row.id,
            &row.to_placement().1,
            stores.objects,
            marker,
            Some(&observation.observation),
        )?;
        progress.file_before = marker;
        registry.write(lock, |tx| tx.update_stopped_rebase(&progress))?;
        hook.on_step(RebaseStep::FileWritten)?;
        return Ok(RebaseOutcome::Stopped(state.clone()));
    }
    if markers_remain(state, &working) {
        return Err(PlacementRefusal {
            path: row.path.to_path_buf(),
            reason: RefusalReason::ConflictMarkers,
        }
        .into());
    }
    let facts = gather_remaining(row, &progress, fs, stores, registry)?;
    let current = progress
        .current_replay
        .ok_or(HistoryRefusal::InvalidFacts)?;
    let original = facts
        .commits
        .get(&current)
        .ok_or(HistoryRefusal::InvalidFacts)?;
    let entry = replay::resolved(original, progress.new_tip, working.clone())?;
    let mut result = replay::replay(&facts, entry.0, working.clone(), &progress.remaining)?;
    result.replayed.insert(0, entry);
    if let Some(stop) = &result.stop
        && stop.marker_lines.is_empty()
    {
        return Err(HistoryRefusal::RebaseConflict(stop.current).into());
    }
    require_observation(&observation, fs)?;
    for (_, commit, bytes) in &result.replayed {
        stores.objects.put(lock, ObjectKind::Blob, bytes)?;
        stores.objects.put_commit(lock, commit)?;
    }
    progress.file_before = stores.objects.put(lock, ObjectKind::Blob, &working)?;
    progress.new_tip = result.tip;
    let mut updated = row.clone();
    if let Some(stop) = result.stop {
        progress.current_replay = Some(stop.current);
        progress.remaining = stop.remaining;
        progress.marker_blob = Some(stores.objects.put(lock, ObjectKind::Blob, &stop.bytes)?);
        let mut state = state.clone();
        state.other = stop.current;
        state.marker_lines = stop.marker_lines;
        updated.conflict = Some(state);
    } else {
        progress.current_replay = None;
        progress.remaining.clear();
        progress.marker_blob = None;
    }
    stores.objects.sync_blobs()?;
    hook.on_step(RebaseStep::ObjectsPublished)?;
    registry.write(lock, |tx| {
        tx.update_stopped_rebase(&progress)?;
        tx.update_placement(&updated)
    })?;
    hook.on_step(RebaseStep::RowWritten)?;
    if let Some(marker) = progress.marker_blob {
        fs.refresh(
            row.id,
            &row.to_placement().1,
            stores.objects,
            marker,
            Some(&observation.observation),
        )?;
        progress.file_before = marker;
        registry.write(lock, |tx| tx.update_stopped_rebase(&progress))?;
        hook.on_step(RebaseStep::FileWritten)?;
        return Ok(RebaseOutcome::Stopped(
            updated.conflict.ok_or(HistoryRefusal::InvalidFacts)?,
        ));
    }
    super::finish::finish(&updated, &progress, fs, stores, registry, lock, hook)
}

fn gather_remaining(
    row: &Placement,
    progress: &StoppedRebase,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<RebaseFacts, HistoryError> {
    let branch = RefName::parse(&progress.branch).map_err(crate::history::RefError::from)?;
    let reset = gather_reset(row.id, &branch, progress.destination, fs, stores, registry)?;
    let mut facts = RebaseFacts {
        reset,
        copy: Some(row.path.clone()),
        stopped: Some(progress.clone()),
        range: Default::default(),
        commits: Default::default(),
        contents: Default::default(),
        left_behind: Vec::new(),
    };
    let current = progress
        .current_replay
        .ok_or(HistoryRefusal::InvalidFacts)?;
    let mut ids = progress.remaining.clone();
    ids.push(current);
    for id in ids.clone() {
        ids.extend(stores.objects.get_commit(id, stores.object_limit)?.parents);
    }
    for id in ids {
        let commit = stores.objects.get_commit(id, stores.object_limit)?;
        let mut bytes = Vec::new();
        stores
            .objects
            .get_to(commit.content, ObjectKind::Blob, &mut bytes)?;
        facts.contents.insert(commit.content, bytes);
        facts.commits.insert(id, commit);
    }
    Ok(facts)
}

fn require_tip(progress: &StoppedRebase, stores: &HistoryStores<'_>) -> Result<(), HistoryError> {
    let branch = RefName::parse(&progress.branch).map_err(crate::history::RefError::from)?;
    let tip = stores.refs.read(RefKind::Branch, &branch)?;
    if tip != Some(progress.original_tip)
        && !(progress.current_replay.is_none() && tip == Some(progress.new_tip))
    {
        return Err(HistoryRefusal::BranchMoved {
            tip,
            ahead: Vec::new(),
        }
        .into());
    }
    Ok(())
}
