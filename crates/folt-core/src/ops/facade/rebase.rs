use crate::home::{Home, HomeLock};
use crate::id::{FolioId, ObjectId};
use crate::ops::history::{
    HistoryError, HistoryRefusal, HistoryStores, RebaseFacts, RebaseOutcome, RebasePlan,
    abort_rebase, apply_rebase, continue_rebase, gather_rebase, judge_rebase,
};
use crate::placement::PlacementOps;
use crate::ref_name::RefName;
use crate::registry::{Placement, Registry};

impl Home {
    /// Gathers history and placements for replay onto `onto`.
    #[expect(
        clippy::too_many_arguments,
        reason = "The selected rebase API retains the owner's inputs and adds the optional copy; this facade forwards them unchanged."
    )]
    pub fn gather_rebase(
        &self,
        id: FolioId,
        branch: &RefName,
        onto: ObjectId,
        from: Option<&Placement>,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &Registry,
    ) -> Result<RebaseFacts, HistoryError> {
        gather_rebase(id, branch, onto, from, fs, stores, registry)
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
    ) -> Result<RebaseOutcome, HistoryError> {
        apply_rebase(plan, fs, stores, registry, lock)
    }
}

impl Home {
    /// Continues the stopped rebase from its recorded copy and durable progress.
    pub fn continue_rebase(
        &self,
        row: &Placement,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &mut Registry,
        lock: &mut HomeLock,
    ) -> Result<RebaseOutcome, HistoryError> {
        continue_rebase(row, fs, stores, registry, lock)
    }

    /// Restores the pre-rebase file; edited conflict content requires discard.
    pub fn abort_rebase(
        &self,
        row: &Placement,
        discard: bool,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &mut Registry,
        lock: &mut HomeLock,
    ) -> Result<(), HistoryError> {
        abort_rebase(row, discard, fs, stores, registry, lock)
    }
}
