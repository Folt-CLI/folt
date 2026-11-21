use std::borrow::Borrow;
use std::path::Path;

use crate::history::{RefError, RefStore};
use crate::home::{Home, HomeLock};
use crate::id::FolioId;
use crate::ops::facts::{PlacementFacts, gather_convert};
use crate::ops::placement::convert::{ConvertPlan, apply_convert, judge_convert};
use crate::ops::placement::move_::MoveTarget;
use crate::ops::placement::rehome::rehome;
use crate::ops::placement::relocate::apply::apply_relocate;
use crate::ops::placement::relocate::judge::{RelocatePlan, judge_relocate};
use crate::ops::placement::relocate::target::{RelocateTarget, gather_relocate_target};
use crate::ops::placement::remove::{RemovePlan, apply_remove, judge_remove};
use crate::ops::placement::repair::{RepairLinkPlan, apply_repair_link, judge_repair_link};
use crate::ops::refusal::PlacementRefusal;
use crate::ops::rules_error::RulesError;
use crate::placement::{PlacementKind, PlacementOps};
use crate::placement_path::PlacementPath;
use crate::registry::{Placement, Registry};
use crate::store::ObjectStore;

impl Home {
    /// Forwards to [`crate::ops::facts::gather_convert`], which owns the rules.
    pub fn gather_convert<R: Borrow<RefStore>>(
        &self,
        placement: &Placement,
        fs: &impl PlacementOps,
        open_refs: impl FnMut(FolioId) -> Result<R, RefError>,
    ) -> Result<PlacementFacts, RulesError> {
        gather_convert(placement, fs, open_refs)
    }

    /// Forwards to [`crate::ops::placement::convert::judge_convert`], which owns the rules.
    pub fn judge_convert(
        &self,
        facts: &PlacementFacts,
        to_kind: PlacementKind,
        target: Option<&MoveTarget>,
        internal: Option<&PlacementFacts>,
    ) -> Result<ConvertPlan, PlacementRefusal> {
        judge_convert(facts, to_kind, target, internal)
    }

    /// Forwards to [`crate::ops::placement::convert::apply_convert`], which owns the rules.
    pub fn apply_convert(
        &self,
        plan: &ConvertPlan,
        fs: &impl PlacementOps,
        objects: &ObjectStore,
        registry: &mut Registry,
        lock: &HomeLock,
    ) -> Result<(), RulesError> {
        apply_convert(plan, fs, objects, registry, lock)
    }

    /// Forwards to [`crate::ops::placement::relocate::target::gather_relocate_target`], which owns the rules.
    pub fn gather_relocate_target(
        &self,
        row: &Placement,
        to: &PlacementPath,
        fs: &impl PlacementOps,
        registry: &Registry,
    ) -> Result<RelocateTarget, RulesError> {
        gather_relocate_target(row, to, fs, registry)
    }

    /// Forwards to [`crate::ops::placement::relocate::judge::judge_relocate`], which owns the rules.
    pub fn judge_relocate(
        &self,
        facts: &PlacementFacts,
        target: &RelocateTarget,
    ) -> Result<RelocatePlan, PlacementRefusal> {
        judge_relocate(facts, target)
    }

    /// Forwards to [`crate::ops::placement::relocate::apply::apply_relocate`], which owns the rules.
    pub fn apply_relocate(
        &self,
        plan: &RelocatePlan,
        fs: &impl PlacementOps,
        registry: &mut Registry,
        lock: &HomeLock,
    ) -> Result<(), RulesError> {
        apply_relocate(plan, fs, registry, lock)
    }

    /// Forwards to [`crate::ops::placement::remove::judge_remove`], which owns the rules.
    pub fn judge_remove(
        &self,
        facts: &PlacementFacts,
        delete_file: bool,
        discard: bool,
    ) -> Result<RemovePlan, PlacementRefusal> {
        judge_remove(facts, delete_file, discard)
    }

    /// Forwards to [`crate::ops::placement::remove::apply_remove`], which owns the rules.
    pub fn apply_remove(
        &self,
        plan: &RemovePlan,
        fs: &impl PlacementOps,
        registry: &mut Registry,
        lock: &HomeLock,
    ) -> Result<(), RulesError> {
        apply_remove(plan, fs, registry, lock)
    }

    /// Forwards to [`crate::ops::placement::repair::judge_repair_link`], which owns the rules.
    pub fn judge_repair_link(
        &self,
        facts: &PlacementFacts,
    ) -> Result<RepairLinkPlan, PlacementRefusal> {
        judge_repair_link(facts)
    }

    /// Forwards to [`crate::ops::placement::repair::apply_repair_link`], which owns the rules.
    pub fn apply_repair_link(
        &self,
        plan: &RepairLinkPlan,
        fs: &impl PlacementOps,
        objects: &ObjectStore,
        registry: &Registry,
        lock: &HomeLock,
    ) -> Result<(), RulesError> {
        apply_repair_link(plan, fs, objects, registry, lock)
    }

    /// Forwards to [`crate::ops::placement::rehome::rehome`], which owns the rules.
    pub fn rehome(
        &self,
        old_home: &Path,
        fs: &impl PlacementOps,
        objects: &ObjectStore,
        registry: &mut Registry,
        lock: &HomeLock,
    ) -> Result<(), RulesError> {
        rehome(old_home, fs, objects, registry, lock)
    }
}
