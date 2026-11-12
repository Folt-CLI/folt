use super::*;
use crate::object::hash_blob;
use crate::test_support::{RelocateFixture, facts, target};

mod convert;
mod crash_recovery;
mod directory_plans;
mod judging;
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
