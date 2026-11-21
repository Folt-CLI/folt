//! Each family of `Home` operation methods forwards to its owner: one success and one refusal.

use folt_core::folio::name::FolioName;
use folt_core::ops::checkout::{CheckoutError, CheckoutStores};
use folt_core::ops::history::{HistoryError, HistoryRefusal, HistoryStores};
use folt_core::ops::prune::PruneOptions;
use folt_core::ops::refusal::RefusalReason;
use folt_core::ops::rename::RenameError;
use folt_core::placement::Drift;
use folt_core::placement_fs::PlacementFs;
use folt_core::ref_name::RefName;

use crate::common::TempHome;
use crate::support::{TestFolio, metadata};

fn name(value: &str) -> FolioName {
    FolioName::parse(value).unwrap()
}

#[test]
fn stage_family_stages_and_unstages_a_copy() {
    let home = TempHome::new();
    let lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let row = folio.copy(&home, &lock);
    let fs = PlacementFs::new(&home.home);
    std::fs::write(row.path.to_path_buf(), b"edited").unwrap();
    home.home
        .stage(
            &row,
            &fs,
            &folio.objects,
            &folio.refs,
            &mut folio.registry,
            &lock,
        )
        .unwrap();
    let staged = folio.registry.placement_at(&row.path).unwrap().unwrap();
    assert!(staged.staged.is_some());
    home.home
        .unstage(&staged, &fs, &mut folio.registry, &lock)
        .unwrap();
    let cleared = folio.registry.placement_at(&row.path).unwrap().unwrap();
    assert!(cleared.staged.is_none());
}

#[test]
fn stage_family_refuses_a_missing_file() {
    let home = TempHome::new();
    let lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let row = folio.copy(&home, &lock);
    std::fs::remove_file(row.path.to_path_buf()).unwrap();
    let result = home.home.stage(
        &row,
        &PlacementFs::new(&home.home),
        &folio.objects,
        &folio.refs,
        &mut folio.registry,
        &lock,
    );
    assert!(result.is_err());
    let unchanged = folio.registry.placement_at(&row.path).unwrap().unwrap();
    assert!(unchanged.staged.is_none());
}

#[test]
fn history_family_commits_and_manages_branches() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let first = folio.commit_internal(&home, &mut lock, b"first");
    let topic = RefName::parse("topic").unwrap();
    home.home
        .create_branch(
            folio.folio.id(),
            &topic,
            first,
            &folio.refs,
            &mut folio.registry,
            &mut lock,
        )
        .unwrap();
    home.home
        .delete_branch(
            folio.folio.id(),
            &topic,
            first,
            &folio.refs,
            &folio.registry,
            &mut lock,
        )
        .unwrap();
    let tag = RefName::parse("v1").unwrap();
    home.home
        .create_tag(&tag, first, None, &folio.stores(), &mut lock)
        .unwrap();
    home.home
        .delete_tag(&tag, first, &folio.refs, &mut lock)
        .unwrap();
}

#[test]
fn history_family_refuses_to_delete_a_branch_in_use() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let first = folio.commit_internal(&home, &mut lock, b"first");
    let result = home.home.delete_branch(
        folio.folio.id(),
        &RefName::parse("main").unwrap(),
        first,
        &folio.refs,
        &folio.registry,
        &mut lock,
    );
    assert!(matches!(
        result,
        Err(HistoryError::Refused(HistoryRefusal::BranchInUse(_)))
    ));
}

#[test]
fn history_family_refuses_a_commit_with_nothing_staged() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    folio.commit_internal(&home, &mut lock, b"first");
    let row = folio.internal();
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let facts = home
        .home
        .gather_commit(
            &row,
            &PlacementFs::new(&home.home),
            &stores,
            &folio.registry,
        )
        .unwrap();
    assert!(matches!(
        home.home.judge_commit(&facts),
        Err(HistoryRefusal::NothingStaged)
    ));
}

#[test]
fn history_family_resets_to_an_earlier_commit() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let first = folio.commit_internal(&home, &mut lock, b"first");
    folio.commit_internal(&home, &mut lock, b"second");
    let fs = PlacementFs::new(&home.home);
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let main = RefName::parse("main").unwrap();
    let facts = home
        .home
        .gather_reset(
            folio.folio.id(),
            &main,
            first,
            &fs,
            &stores,
            &folio.registry,
        )
        .unwrap();
    let plan = home.home.judge_reset(&facts, false).unwrap();
    home.home
        .apply_reset(&plan, &fs, &stores, &mut folio.registry, &mut lock)
        .unwrap();
    assert_eq!(folio.internal().commit_id, Some(first));
    let _ = metadata();
}

#[test]
fn checkout_family_checks_a_branch_out() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let row = folio.copy(&home, &lock);
    let commit = folio.commit_object(&lock, b"branch content");
    let topic = RefName::parse("topic").unwrap();
    home.home
        .create_branch(
            folio.folio.id(),
            &topic,
            commit,
            &folio.refs,
            &mut folio.registry,
            &mut lock,
        )
        .unwrap();
    let target = home
        .home
        .resolve_checkout_target(&folio.refs, &folio.objects, &topic, 16384)
        .unwrap();
    assert_eq!(target.commit, commit);
    let stores = CheckoutStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    home.home
        .checkout_placement(
            &row,
            &topic,
            false,
            &PlacementFs::new(&home.home),
            &stores,
            &mut folio.registry,
            &lock,
        )
        .unwrap();
    assert_eq!(
        std::fs::read(row.path.to_path_buf()).unwrap(),
        b"branch content"
    );
}

#[test]
fn checkout_family_refuses_a_missing_branch() {
    let home = TempHome::new();
    let lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let row = folio.copy(&home, &lock);
    let stores = CheckoutStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let result = home.home.checkout_placement(
        &row,
        &RefName::parse("absent").unwrap(),
        false,
        &PlacementFs::new(&home.home),
        &stores,
        &mut folio.registry,
        &lock,
    );
    assert!(matches!(result, Err(CheckoutError::MissingBranch(_))));
}

#[test]
fn placement_family_removes_a_pristine_copy_row() {
    let home = TempHome::new();
    let lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let row = folio.copy(&home, &lock);
    let fs = PlacementFs::new(&home.home);
    let facts = home
        .home
        .gather_facts(std::slice::from_ref(&row), &fs, |_| Ok(&folio.refs))
        .unwrap();
    let plan = home.home.judge_remove(&facts[0], false, false).unwrap();
    home.home
        .apply_remove(&plan, &fs, &mut folio.registry, &lock)
        .unwrap();
    assert!(folio.registry.placement_at(&row.path).unwrap().is_none());
}

#[test]
fn placement_family_refuses_to_repair_a_copy() {
    let home = TempHome::new();
    let lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let row = folio.copy(&home, &lock);
    let fs = PlacementFs::new(&home.home);
    let facts = home
        .home
        .gather_facts(std::slice::from_ref(&row), &fs, |_| Ok(&folio.refs))
        .unwrap();
    let refusal = home.home.judge_repair_link(&facts[0]).unwrap_err();
    assert_eq!(refusal.reason, RefusalReason::OtherLink);
}

#[test]
fn register_family_plans_and_applies_a_file() {
    let home = TempHome::new();
    let path = home.root().join("note.md");
    std::fs::write(&path, b"body").unwrap();
    let plan = home.home.plan_register([&path]).unwrap();
    assert_eq!(plan.summary.new, 1);
    let applied = home.home.apply_register(&plan, false).unwrap();
    assert_eq!(applied.summary.new, 1);
    let again = home.home.plan_register([&path]).unwrap();
    assert_eq!(again.summary.already_registered, 1);
}

#[test]
fn register_family_refuses_a_missing_path() {
    let home = TempHome::new();
    let path = home.root().join("absent.md");
    let plan = home.home.plan_register([&path]).unwrap();
    assert_eq!(plan.summary.refused, 1);
    assert_eq!(plan.summary.new, 0);
}

#[test]
fn rename_family_renames_a_folio() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    folio.commit_internal(
        &home,
        &mut lock,
        b"---\r\nfolio-name: 'old' # keep\r\ntitle: keep\r\n---\r\nbody\n",
    );
    folio
        .registry
        .write(&lock, |tx| tx.set_name(folio.folio.id(), &name("old")))
        .unwrap();
    let fs = PlacementFs::new(&home.home);
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let plan = home
        .home
        .plan_rename(&name("old"), &name("new"), &fs, &stores, &folio.registry)
        .unwrap();
    home.home
        .apply_rename(
            &plan,
            metadata(),
            &fs,
            &stores,
            &mut folio.registry,
            &mut lock,
            &mut |_, _| Ok(false),
        )
        .unwrap();
    assert_eq!(
        folio.registry.folio_name(folio.folio.id()).unwrap(),
        Some(name("new"))
    );
}

#[test]
fn rename_family_refuses_an_unknown_name() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let result = home.home.plan_rename(
        &name("old"),
        &name("new"),
        &PlacementFs::new(&home.home),
        &stores,
        &folio.registry,
    );
    assert!(matches!(result, Err(RenameError::UnknownName(_))));
}

#[test]
fn prune_family_drops_a_gone_copy() {
    let home = TempHome::new();
    let lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let row = folio.copy(&home, &lock);
    std::fs::remove_file(row.path.to_path_buf()).unwrap();
    let fs = PlacementFs::new(&home.home);
    let plan = home.home.gather_prune(&folio.registry, &fs, &[]).unwrap();
    let report = home
        .home
        .apply_prune(&plan, &fs, &mut folio.registry, &lock, false)
        .unwrap();
    assert_eq!(report.dropped.len(), 1);
    assert!(folio.registry.placement_at(&row.path).unwrap().is_none());
}

#[test]
fn prune_family_keeps_a_present_copy() {
    let home = TempHome::new();
    let lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let row = folio.copy(&home, &lock);
    let report = home
        .home
        .prune_placements(
            &mut folio.registry,
            &PlacementFs::new(&home.home),
            &lock,
            &[],
            PruneOptions {
                apply: true,
                confirm_mass_deletion: false,
            },
        )
        .unwrap();
    assert!(report.dropped.is_empty());
    assert!(folio.registry.placement_at(&row.path).unwrap().is_some());
}

#[test]
fn status_family_reports_in_sync_and_gone_copies() {
    let home = TempHome::new();
    let lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let row = folio.copy(&home, &lock);
    let fs = PlacementFs::new(&home.home);
    let status = home
        .home
        .gather_status(
            std::slice::from_ref(&row),
            &fs,
            |_| Ok(&folio.refs),
            &folio.registry,
        )
        .unwrap();
    assert_eq!(status[0].drift, Drift::InSync);
    std::fs::remove_file(row.path.to_path_buf()).unwrap();
    let facts = home
        .home
        .gather_facts(std::slice::from_ref(&row), &fs, |_| Ok(&folio.refs))
        .unwrap();
    assert_eq!(home.home.judge_status(&facts)[0].drift, Drift::Gone);
    let _ = home.home.judge_working(&facts[0]);
}
