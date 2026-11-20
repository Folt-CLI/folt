use super::apply::{RelocateStep, apply_relocate, apply_relocate_steps};
use super::judge::{RelocatePlan, judge_relocate};
use super::target::{RelocateTarget, gather_relocate_target};
use crate::ops::refusal::RefusalReason;
use crate::placement::{Observation, PlacementError, PlacementKind, PlacementOps};
use crate::registry::Registry;
use crate::test_support::{RelocateFixture, facts};

mod crash_recovery;
mod directory_plans;
mod refusals;
mod relocation_failures;

fn assert_move_rows(plan: &RelocatePlan, registry: &Registry, old: bool, new: bool) {
    for (source, destination) in &plan.target.rows {
        assert_eq!(
            registry.placement_at(&source.path).unwrap(),
            old.then(|| source.clone())
        );
        assert_eq!(
            registry.placement_at(&destination.path).unwrap(),
            new.then(|| destination.clone())
        );
    }
}
