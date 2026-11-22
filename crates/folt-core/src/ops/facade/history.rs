use crate::history::RefStore;
use crate::home::{Home, HomeLock};
use crate::id::{FolioId, ObjectId};
use crate::object::Tag;
use crate::ops::history::{
    CommitFacts, CommitMetadata, CommitPlan, HistoryError, HistoryRefusal, HistoryStores,
    RebaseFacts, RebasePlan, ResetFacts, ResetPlan, apply_commit, apply_rebase, apply_reset,
    create_branch, create_tag, delete_branch, delete_tag, gather_commit, gather_rebase,
    gather_reset, judge_commit, judge_rebase, judge_reset,
};
use crate::placement::PlacementOps;
use crate::ref_name::RefName;
use crate::registry::{Placement, Registry};

impl Home {
    /// Forwards to [`crate::ops::history::create_branch`], which owns the rules.
    pub fn create_branch(
        &self,
        id: FolioId,
        name: &RefName,
        commit: ObjectId,
        refs: &RefStore,
        registry: &mut Registry,
        lock: &mut HomeLock,
    ) -> Result<(), HistoryError> {
        create_branch(id, name, commit, refs, registry, lock)
    }

    /// Forwards to [`crate::ops::history::delete_branch`], which owns the rules.
    pub fn delete_branch(
        &self,
        id: FolioId,
        name: &RefName,
        old: ObjectId,
        refs: &RefStore,
        registry: &Registry,
        lock: &mut HomeLock,
    ) -> Result<(), HistoryError> {
        delete_branch(id, name, old, refs, registry, lock)
    }

    /// Forwards to [`crate::ops::history::create_tag`], which owns the rules.
    pub fn create_tag(
        &self,
        name: &RefName,
        commit: ObjectId,
        annotation: Option<&Tag>,
        stores: &HistoryStores<'_>,
        lock: &mut HomeLock,
    ) -> Result<ObjectId, HistoryError> {
        create_tag(name, commit, annotation, stores, lock)
    }

    /// Forwards to [`crate::ops::history::delete_tag`], which owns the rules.
    pub fn delete_tag(
        &self,
        name: &RefName,
        old: ObjectId,
        refs: &RefStore,
        lock: &mut HomeLock,
    ) -> Result<(), HistoryError> {
        delete_tag(name, old, refs, lock)
    }

    /// Forwards to [`crate::ops::history::gather_commit`], which owns the rules.
    pub fn gather_commit(
        &self,
        row: &Placement,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &Registry,
    ) -> Result<CommitFacts, HistoryError> {
        gather_commit(row, fs, stores, registry)
    }

    /// Forwards to [`crate::ops::history::judge_commit`], which owns the rules.
    pub fn judge_commit(&self, facts: &CommitFacts) -> Result<CommitPlan, HistoryRefusal> {
        judge_commit(facts)
    }

    /// Forwards to [`crate::ops::history::apply_commit`], which owns the rules.
    pub fn apply_commit(
        &self,
        plan: &CommitPlan,
        metadata: CommitMetadata,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &mut Registry,
        lock: &mut HomeLock,
    ) -> Result<ObjectId, HistoryError> {
        apply_commit(plan, metadata, fs, stores, registry, lock)
    }

    /// Forwards to [`crate::ops::history::gather_reset`], which owns the rules.
    pub fn gather_reset(
        &self,
        id: FolioId,
        branch: &RefName,
        commit: ObjectId,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &Registry,
    ) -> Result<ResetFacts, HistoryError> {
        gather_reset(id, branch, commit, fs, stores, registry)
    }

    /// Forwards to [`crate::ops::history::judge_reset`], which owns the rules.
    pub fn judge_reset(
        &self,
        facts: &ResetFacts,
        discard: bool,
    ) -> Result<ResetPlan, HistoryRefusal> {
        judge_reset(facts, discard)
    }

    /// Forwards to [`crate::ops::history::apply_reset`], which owns the rules.
    pub fn apply_reset(
        &self,
        plan: &ResetPlan,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &mut Registry,
        lock: &mut HomeLock,
    ) -> Result<(), HistoryError> {
        apply_reset(plan, fs, stores, registry, lock)
    }
}

impl Home {
    /// Gathers history and placements for replay onto `onto`.
    pub fn gather_rebase(
        &self,
        id: FolioId,
        branch: &RefName,
        onto: ObjectId,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &Registry,
    ) -> Result<RebaseFacts, HistoryError> {
        gather_rebase(id, branch, onto, fs, stores, registry)
    }

    /// Judges replay and delegates placement decisions to reset.
    pub fn judge_rebase(
        &self,
        facts: &RebaseFacts,
        discard: bool,
    ) -> Result<RebasePlan, HistoryRefusal> {
        judge_rebase(facts, discard)
    }

    /// Publishes replay objects and applies the judged reset.
    pub fn apply_rebase(
        &self,
        plan: &RebasePlan,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &mut Registry,
        lock: &mut HomeLock,
    ) -> Result<ObjectId, HistoryError> {
        apply_rebase(plan, fs, stores, registry, lock)
    }
}
