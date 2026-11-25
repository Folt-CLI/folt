use super::*;
use crate::config::env::Environment;
use crate::id::FolioId;
use crate::object::hash_blob;
use crate::ops::status::{Edit, judge_working};
use crate::placement::StagedBlob;
use crate::placement::{Observation, PlacementKind};
use crate::test_support::{TempDir, facts, target};

fn commit_facts() -> CommitFacts {
    let mut working = facts();
    working.placement.staged = Some(StagedBlob {
        hash: hash_blob(b"staged"),
        size: 6,
    });
    CommitFacts {
        working,
        link: None,
        internal: None,
        placements: vec![],
        tip_commit: None,
    }
}

#[test]
fn commit_refuses_links_missing_stage_and_unusable_files() {
    let mut facts = commit_facts();
    facts.working.placement.kind = PlacementKind::Link;
    assert!(matches!(judge_commit(&facts), Err(HistoryRefusal::Link(_))));
    facts.working.placement.kind = PlacementKind::Copy;
    facts.working.placement.staged = None;
    assert_eq!(
        judge_commit(&facts).unwrap_err(),
        HistoryRefusal::NothingStaged
    );
    let mut facts = commit_facts();
    facts.working.observation = Observation::Missing;
    assert!(matches!(
        judge_commit(&facts),
        Err(HistoryRefusal::Unusable(_))
    ));
}

#[test]
fn commit_refuses_moved_branch_and_invalid_internal_head() {
    let mut facts = commit_facts();
    facts.working.tip = Some(hash_blob(b"advanced"));
    assert!(matches!(
        judge_commit(&facts),
        Err(HistoryRefusal::BranchMoved { .. })
    ));
    facts.working.placement.kind = PlacementKind::Internal;
    assert_eq!(
        judge_commit(&facts).unwrap_err(),
        HistoryRefusal::InvalidFacts
    );
}

#[test]
fn main_copy_requires_clean_internal_file() {
    let mut facts = commit_facts();
    facts.working.placement.head = "main".into();
    assert_eq!(
        judge_commit(&facts).unwrap_err(),
        HistoryRefusal::MissingInternal
    );
    let mut internal = facts.working.clone();
    internal.placement.kind = PlacementKind::Internal;
    internal.placement_hash = Some(hash_blob(b"edited"));
    facts.internal = Some(internal);
    assert!(matches!(
        judge_commit(&facts),
        Err(HistoryRefusal::InternalEdited(_))
    ));
    facts.internal.as_mut().unwrap().placement.id = FolioId::from_bytes([2; 16]);
    assert_eq!(
        judge_commit(&facts).unwrap_err(),
        HistoryRefusal::InvalidFacts
    );
}

#[test]
fn main_copy_refuses_clean_internal_with_stage() {
    let mut facts = commit_facts();
    facts.working.placement.head = "main".into();
    let mut internal = facts.working.clone();
    internal.placement.kind = PlacementKind::Internal;
    assert_eq!(judge_working(&internal).edit, Edit::Clean);
    facts.internal = Some(internal);
    let refusal = judge_commit(&facts).unwrap_err();
    assert!(matches!(refusal, HistoryRefusal::InternalEdited(_)));
    assert!(
        refusal
            .to_string()
            .contains("commit through a link or the internal row, or checkout main with discard")
    );
    facts.internal.as_mut().unwrap().placement.staged = None;
    assert!(judge_commit(&facts).is_ok());
}

#[test]
fn reset_requires_discard_for_edits() {
    let mut placement = facts();
    placement.placement_hash = Some(hash_blob(b"edits"));
    let reset = ResetFacts {
        id: placement.placement.id,
        target: target(),
        old: placement.tip.unwrap(),
        placements: vec![placement],
    };
    assert!(matches!(
        judge_reset(&reset, false),
        Err(HistoryRefusal::Move(_))
    ));
    assert!(judge_reset(&reset, true).is_ok());
}

#[test]
fn branch_sync_failure_leaves_cached_rows_untouched_and_harmless() {
    use crate::config::formats::OBJECT_ALGORITHM;
    use crate::folio::dir::{Durability, FolioSpec, create_folio};
    use crate::folio::format::FolioFormat;
    use crate::folio::row::NewFolio;
    use crate::home::Home;
    use crate::layout::FOLIO_LAYOUT_VERSION;

    let directory = TempDir::new().unwrap();
    let root = directory.path().join("home");
    {
        let home = Home::open_or_create(root.clone(), &Environment::default()).unwrap();
        let mut lock = home.try_lock().unwrap();
        let folio = create_folio(
            home.path(),
            &lock,
            FolioSpec {
                content: b"initial".to_vec(),
            },
            Durability::None,
        )
        .unwrap();
        let format = FolioFormat {
            layout: FOLIO_LAYOUT_VERSION,
            algorithm: OBJECT_ALGORITHM.into(),
        };
        let fs =
            crate::fsx::fault::FaultFileSystem::new().sync_failure(&folio.path().join("refs"), 0);
        let refs = RefStore::open_with(&folio, &format, Box::new(fs)).unwrap();
        let mut registry = Registry::open_or_create(&home, &lock).unwrap();
        let target = FolioId::generate();
        registry
            .write(&lock, |tx| {
                for id in [folio.id(), target] {
                    tx.add_folio(&NewFolio {
                        id,
                        format: format.clone(),
                    })?;
                }
                tx.rebuild_references(
                    folio.id(),
                    "topic",
                    &[crate::front_matter::Reference {
                        alias: "edge".into(),
                        name: None,
                        id: Some(target),
                        pin: None,
                    }],
                )
            })
            .unwrap();
        let topic = RefName::parse("topic").unwrap();
        let old = hash_blob(b"tip");
        std::fs::create_dir_all(folio.path().join("refs/heads")).unwrap();
        // Only deletion reads this ref; it needs no object lookup.
        std::fs::write(folio.path().join("refs/heads/topic"), format!("{old}\n")).unwrap();
        let result = delete_branch(folio.id(), &topic, old, &refs, &registry, &mut lock);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("injected directory sync failure")
        );
        assert_eq!(refs.read(RefKind::Branch, &topic).unwrap(), None);
        assert_eq!(registry.references(folio.id(), "topic").unwrap().len(), 1);
        registry
            .write(&lock, |tx| {
                tx.remove_folio(target, &mut |parent, branch| {
                    assert_eq!(parent, folio.id());
                    Ok(refs
                        .read(RefKind::Branch, &RefName::parse(branch).unwrap())
                        .unwrap()
                        .is_some())
                })
            })
            .unwrap();
        assert!(registry.references(folio.id(), "topic").unwrap().is_empty());
    }
}

#[test]
fn branch_with_any_placement_cannot_be_deleted() {
    assert!(judge_delete_branch(&[]).is_ok());
    let mut row = facts().placement;
    assert_eq!(
        judge_delete_branch(&[row.clone()]).unwrap_err(),
        HistoryRefusal::BranchInUse(vec![row.path.to_path_buf()])
    );
    row.kind = PlacementKind::Internal;
    row.head = "main".into();
    assert!(matches!(
        judge_delete_branch(&[row]),
        Err(HistoryRefusal::BranchInUse(_))
    ));
}

fn replay_commit(
    facts: &mut RebaseFacts,
    content: &[u8],
    parents: &[ObjectId],
    message: &str,
) -> ObjectId {
    use crate::object::{Author, Commit, CommitTime, ObjectKind, hash_object};
    let commit = Commit {
        content: hash_blob(content),
        parents: parents.to_vec(),
        author: Author::anonymous(),
        time: CommitTime::from_unix_seconds(123).unwrap(),
        message: message.into(),
        body: Some("body\n".into()),
        group: Some("group".into()),
    };
    let id = hash_object(ObjectKind::Commit, &commit.encode_body().unwrap());
    facts.contents.insert(commit.content, content.to_vec());
    facts.commits.insert(id, commit);
    id
}

fn rebase_facts() -> (RebaseFacts, ObjectId, ObjectId) {
    let placement = facts();
    let mut facts = RebaseFacts {
        copy: None,
        stopped: None,
        reset: ResetFacts {
            id: placement.placement.id,
            target: target(),
            old: placement.tip.unwrap(),
            placements: vec![],
        },
        range: Default::default(),
        commits: Default::default(),
        contents: Default::default(),
        left_behind: vec![],
    };
    let root = replay_commit(&mut facts, b"a\nb\nc\n", &[], "root");
    let onto = replay_commit(&mut facts, b"A\nb\nc\n", &[root], "onto");
    facts.reset.target.commit = onto;
    facts.reset.target.content = facts.commits[&onto].content;
    (facts, root, onto)
}

#[test]
fn rebase_is_deterministic_drops_merges_and_chains_metadata() {
    let (mut facts, root, onto) = rebase_facts();
    let left = replay_commit(&mut facts, b"a\nB\nc\n", &[root], "left");
    let right = replay_commit(&mut facts, b"a\nb\nC\n", &[root], "right");
    let merge = replay_commit(
        &mut facts,
        b"merge resolution omitted\n",
        &[left, right],
        "merge",
    );
    let after = replay_commit(&mut facts, b"merge resolution omitted\n", &[merge], "after");
    facts.range.extend([left, right, merge, after]);
    facts.reset.old = after;
    let plan = judge_rebase(&facts, false).unwrap();
    let mut reordered = facts.clone();
    reordered.range = [after, merge, right, left].into_iter().collect();
    reordered.commits = facts
        .commits
        .iter()
        .map(|(id, commit)| (*id, commit.clone()))
        .collect();
    assert_eq!(judge_rebase(&reordered, false).unwrap(), plan);
    let mut order = [left, right];
    order.sort_by_key(|id| *id.as_bytes());
    let mut expected_content = b"A\nB\nC\n".to_vec();
    let mut parent = onto;
    for original_id in order.into_iter().chain([after]) {
        let mut expected = facts.commits[&original_id].clone();
        expected.parents = vec![parent];
        let content = if original_id == after {
            expected_content.clone()
        } else if original_id == left && parent == onto {
            b"A\nB\nc\n".to_vec()
        } else if original_id == right && parent == onto {
            b"A\nb\nC\n".to_vec()
        } else {
            expected_content.clone()
        };
        expected.content = hash_blob(&content);
        parent = crate::object::hash_object(ObjectKind::Commit, &expected.encode_body().unwrap());
        expected_content = b"A\nB\nC\n".to_vec();
    }
    assert_eq!(plan.tip(), parent);
}

#[test]
fn rebase_refuses_conflicts_and_parentless_commits() {
    let (mut facts, root, _) = rebase_facts();
    let conflict = replay_commit(&mut facts, b"different\nb\nc\n", &[root], "conflict");
    facts.range.insert(conflict);
    assert_eq!(
        judge_rebase(&facts, false).unwrap_err(),
        HistoryRefusal::RebaseConflict(conflict)
    );
    facts.range = [root].into_iter().collect();
    assert_eq!(
        judge_rebase(&facts, false).unwrap_err(),
        HistoryRefusal::RebaseRoot(root)
    );
    facts.range.clear();
    assert_eq!(
        judge_rebase(&facts, false).unwrap().tip(),
        facts.reset.target.commit
    );
}

#[test]
fn rebase_reports_left_behind_and_refuses_inconsistent_placements() {
    let (mut facts, root, _) = rebase_facts();
    let left = replay_commit(&mut facts, b"a\nB\nc\n", &[root], "left");
    facts.range.insert(left);
    facts.reset.old = left;
    facts.left_behind.push(LeftBehind {
        kind: RefKind::Tag,
        name: RefName::parse("release").unwrap(),
        commit: left,
    });
    assert_eq!(
        judge_rebase(&facts, false).unwrap().left_behind(),
        facts.left_behind
    );
    let mut placement = crate::test_support::facts();
    placement.placement.id = FolioId::from_bytes([99; 16]);
    facts.reset.placements.push(placement);
    assert_eq!(
        judge_rebase(&facts, true).unwrap_err(),
        HistoryRefusal::InvalidFacts
    );
}

#[test]
fn rebase_empty_range_still_judges_placements_against_final_content() {
    let (mut rebase, _, onto) = rebase_facts();
    let mut placement = facts();
    placement.placement_hash = Some(rebase.reset.target.content);
    rebase.reset.placements.push(placement);
    assert_eq!(judge_rebase(&rebase, false).unwrap().tip(), onto);
    rebase.reset.placements[0].placement_hash = Some(hash_blob(b"unrelated edits"));
    assert!(matches!(
        judge_rebase(&rebase, false),
        Err(HistoryRefusal::Move(_))
    ));
    assert!(judge_rebase(&rebase, true).is_ok());
    rebase.reset.placements[0].tip = None;
    assert_eq!(
        judge_rebase(&rebase, true).unwrap_err(),
        HistoryRefusal::InvalidFacts
    );
}

fn merge_facts() -> merge::MergeFacts {
    let mut history = commit_facts();
    history.working.placement.staged = None;
    merge::MergeFacts {
        history,
        other: Some(hash_blob(b"other tip")),
        base: crate::history::MergeBase::One(hash_blob(b"merge base")),
        ancestors_binary_conflict: false,
        merged: crate::merge::merge3(b"a\nb\nc\n", b"A\nb\nc\n", b"a\nb\nC\n"),
        validation: Ok(()),
    }
}

#[test]
fn merge_refuses_link_unusable_stage_edits_and_moved_branch_in_order() {
    let mut facts = merge_facts();
    facts.history.working.placement.kind = PlacementKind::Link;
    facts.history.working.observation = Observation::Missing;
    assert!(matches!(
        merge::judge_merge(&facts),
        Err(HistoryRefusal::Link(_))
    ));
    facts.history.working.placement.kind = PlacementKind::Copy;
    facts.history.working.placement.staged = Some(StagedBlob {
        hash: hash_blob(b"stage"),
        size: 5,
    });
    assert!(matches!(
        merge::judge_merge(&facts),
        Err(HistoryRefusal::Unusable(_))
    ));
    facts.history.working.observation = crate::test_support::facts().observation;
    facts.history.working.tip = Some(hash_blob(b"moved"));
    assert!(matches!(
        merge::judge_merge(&facts),
        Err(HistoryRefusal::MergeEdited(_))
    ));
    facts.history.working.placement.staged = None;
    facts.history.working.placement_hash = Some(hash_blob(b"edits"));
    assert!(matches!(
        merge::judge_merge(&facts),
        Err(HistoryRefusal::MergeEdited(_))
    ));
    facts.history.working.placement_hash = crate::test_support::facts().placement_hash;
    facts.other = None;
    assert!(matches!(
        merge::judge_merge(&facts),
        Err(HistoryRefusal::BranchMoved { .. })
    ));
    facts.history.working.tip = facts.history.working.placement.commit_id;
    assert_eq!(
        merge::judge_merge(&facts).unwrap_err(),
        HistoryRefusal::MissingBranch
    );
}

#[test]
fn merge_refuses_already_merged_no_base_conflict_then_validation() {
    use crate::history::MergeBase;
    let mut facts = merge_facts();
    facts.other = facts.history.working.tip;
    facts.base = MergeBase::None;
    assert_eq!(
        merge::judge_merge(&facts).unwrap_err(),
        HistoryRefusal::MergeAlreadyMerged
    );
    facts = merge_facts();
    facts.base = MergeBase::One(facts.other.unwrap());
    assert_eq!(
        merge::judge_merge(&facts).unwrap_err(),
        HistoryRefusal::MergeAlreadyMerged
    );
    facts.base = MergeBase::None;
    assert_eq!(
        merge::judge_merge(&facts).unwrap_err(),
        HistoryRefusal::MergeNoBase
    );
    facts.base = MergeBase::Several(vec![hash_blob(b"a"), hash_blob(b"b")]);
    assert!(merge::judge_merge(&facts).is_ok());
    facts.base = merge_facts().base;
    facts.merged = crate::merge::merge3(b"a\nb\nc\n", b"A\nb\nc\n", b"different\nb\nc\n");
    facts.validation = Err("invalid".into());
    assert!(matches!(
        merge::judge_merge(&facts),
        Ok(MergePlan::Conflicted { .. })
    ));
    facts.history.working.placement.kind = PlacementKind::Internal;
    assert_eq!(
        merge::judge_merge(&facts).unwrap_err(),
        HistoryRefusal::MergeConflict
    );
    facts.history.working.placement.kind = PlacementKind::Copy;
    facts.merged = crate::merge::merge3(b"base", b"ours\0", b"theirs");
    assert_eq!(
        merge::judge_merge(&facts).unwrap_err(),
        HistoryRefusal::MergeConflict
    );
    facts.merged = merge_facts().merged;
    assert_eq!(
        merge::judge_merge(&facts).unwrap_err(),
        HistoryRefusal::MergeInvalid("invalid".into())
    );
}

#[test]
fn merge_accepts_fast_forward_and_refuses_edited_internal() {
    let mut facts = merge_facts();
    facts.base = crate::history::MergeBase::One(facts.history.working.tip.unwrap());
    facts.merged = crate::merge::merge3(b"ours", b"ours", b"theirs");
    assert!(merge::judge_merge(&facts).is_ok());
    facts.history.working.placement.head = "main".into();
    assert_eq!(
        merge::judge_merge(&facts).unwrap_err(),
        HistoryRefusal::MissingInternal
    );
    let mut internal = facts.history.working.clone();
    internal.placement.kind = PlacementKind::Internal;
    internal.placement_hash = Some(hash_blob(b"internal edit"));
    facts.history.internal = Some(internal);
    assert!(matches!(
        merge::judge_merge(&facts),
        Err(HistoryRefusal::InternalEdited(_))
    ));
}

#[test]
fn merge_refuses_binary_ancestors_before_content_conflict_and_validation() {
    let mut facts = merge_facts();
    facts.base = crate::history::MergeBase::Several(vec![hash_blob(b"a"), hash_blob(b"b")]);
    facts.ancestors_binary_conflict = true;
    facts.merged = crate::merge::merge3(b"base", b"ours", b"theirs");
    facts.validation = Err("invalid".into());
    let refusal = merge::judge_merge(&facts).unwrap_err();
    assert_eq!(refusal, HistoryRefusal::MergeBaseBinaryConflict);
    assert_eq!(
        refusal.to_string(),
        "merge base ancestors have a binary conflict"
    );
    facts.base = crate::history::MergeBase::None;
    assert_eq!(
        merge::judge_merge(&facts).unwrap_err(),
        HistoryRefusal::MergeNoBase
    );
}

pub(super) fn conflict_fixture(
    home: &crate::home::Home,
    lock: &mut crate::home::HomeLock,
) -> (Registry, crate::store::ObjectStore, RefStore, Placement) {
    use crate::folio::{
        dir::{Durability, FolioSpec, create_folio},
        format::FolioFormat,
        row::NewFolio,
    };
    use crate::object::{Author, Commit, CommitTime};
    use crate::placement::PlacementOps;
    let folio = create_folio(
        home.path(),
        lock,
        FolioSpec {
            content: b"base\n".to_vec(),
        },
        Durability::None,
    )
    .unwrap();
    let format = FolioFormat {
        layout: crate::layout::FOLIO_LAYOUT_VERSION,
        algorithm: crate::config::formats::OBJECT_ALGORITHM.into(),
    };
    let objects = crate::store::ObjectStore::open(&folio, &format).unwrap();
    let refs = RefStore::open(&folio, &format).unwrap();
    let make_commit = |bytes: &[u8], parents| {
        let content = objects.put(lock, ObjectKind::Blob, bytes).unwrap();
        objects
            .put_commit(
                lock,
                &Commit {
                    content,
                    parents,
                    author: Author::anonymous(),
                    time: CommitTime::from_unix_seconds(0).unwrap(),
                    message: "fixture".into(),
                    body: None,
                    group: None,
                },
            )
            .unwrap()
    };
    let base = make_commit(b"base\n", vec![]);
    let ours = make_commit(b"ours\n", vec![base]);
    let theirs = make_commit(b"theirs\n", vec![base]);
    for (branch, tip) in [("main", ours), ("topic", theirs)] {
        refs.update(
            lock,
            RefKind::Branch,
            &RefName::parse(branch).unwrap(),
            tip,
            crate::history::Expect::Absent,
        )
        .unwrap();
    }
    let mut row = facts().placement;
    row.id = folio.id();
    row.path = crate::placement_path::PlacementPath::resolve(home.path().join("copy.txt")).unwrap();
    row.head = "main".into();
    row.commit_id = Some(ours);
    row.base = crate::placement_fs::PlacementFs::new(home)
        .create(
            row.id,
            &row.to_placement().1,
            &objects,
            hash_blob(b"ours\n"),
        )
        .unwrap()
        .base;
    let mut registry = Registry::open_or_create(home, lock).unwrap();
    registry
        .write(lock, |tx| {
            tx.add_folio(&NewFolio { id: row.id, format })?;
            tx.insert_placement(&row)
        })
        .unwrap();
    (registry, objects, refs, row)
}

#[test]
fn conflict_failure_after_file_write_leaves_no_state_and_discard_checkout_restores_copy() {
    use crate::placement::PlacementOps;
    let home = crate::test_support::TempHome::new().unwrap();
    let mut lock = home.home.try_lock().unwrap();
    let (mut registry, objects, refs, row) = conflict_fixture(&home.home, &mut lock);
    let fs = crate::placement_fs::PlacementFs::new(&home.home);
    let stores = HistoryStores {
        objects: &objects,
        refs: &refs,
        object_limit: 16384,
    };
    let topic = RefName::parse("topic").unwrap();
    let metadata = || CommitMetadata {
        author: crate::object::Author::anonymous(),
        time: crate::object::CommitTime::from_unix_seconds(0).unwrap(),
        message: "merge".into(),
        body: None,
        group: None,
    };
    let result = merge::merge_branch_steps(
        &row,
        MergeRequest {
            other: &topic,
            metadata: metadata(),
        },
        &fs,
        &stores,
        &mut registry,
        &mut lock,
        |step| {
            assert_eq!(step, merge::conflict::ConflictStep::FileWritten);
            Err(HistoryRefusal::InvalidFacts.into())
        },
    );
    assert!(matches!(
        result,
        Err(HistoryError::Refused(HistoryRefusal::InvalidFacts))
    ));
    let crate::merge::MergeResult::Conflicted {
        merged: Some(expected),
        ..
    } = crate::merge::merge3(b"base\n", b"ours\n", b"theirs\n")
    else {
        panic!("expected text conflict")
    };
    assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), expected);
    assert_eq!(registry.placement_at(&row.path).unwrap(), Some(row.clone()));
    assert_eq!(
        home.home.conflict_state(&row.path, &registry).unwrap(),
        None
    );
    assert_eq!(
        refs.read(RefKind::Branch, &RefName::parse("main").unwrap())
            .unwrap(),
        row.commit_id
    );
    let request = MergeRequest {
        other: &topic,
        metadata: metadata(),
    };
    assert!(matches!(
        home.home
            .merge_branch(&row, request, &fs, &stores, &mut registry, &mut lock),
        Err(HistoryError::Refused(HistoryRefusal::MergeEdited(_)))
    ));
    home.home
        .checkout_placement(
            &row,
            &RefName::parse("main").unwrap(),
            true,
            &fs,
            &crate::ops::checkout::CheckoutStores {
                objects: &objects,
                refs: &refs,
                object_limit: 16384,
            },
            &mut registry,
            &lock,
        )
        .unwrap();
    assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), b"ours\n");
    let restored = registry.placement_at(&row.path).unwrap().unwrap();
    assert_eq!(restored.commit_id, row.commit_id);
    assert_eq!(restored.conflict, None);
    assert_eq!(
        fs.hash_placement(&restored.to_placement().1).unwrap(),
        hash_blob(b"ours\n")
    );
}

#[test]
fn commit_conflict_refusal_precedes_stage_and_branch_checks() {
    let mut facts = commit_facts();
    facts.working = crate::test_support::conflicted_facts();
    facts.working.tip = Some(hash_blob(b"advanced"));
    let refusal = judge_commit(&facts).unwrap_err();
    assert!(
        matches!(&refusal, HistoryRefusal::Move(error) if error.reason == crate::ops::refusal::RefusalReason::Conflicted)
    );
    assert!(
        refusal.to_string().contains(
            &facts
                .working
                .placement
                .path
                .to_path_buf()
                .display()
                .to_string()
        )
    );
}

#[test]
fn reset_skips_conflicted_copy_and_plans_ordinary_copy() {
    let conflicted = crate::test_support::conflicted_facts();
    let mut ordinary = facts();
    ordinary.placement.path = crate::placement_path::PlacementPath::resolve(
        std::env::temp_dir().join("folt-reset-ordinary"),
    )
    .unwrap();
    let reset = ResetFacts {
        id: conflicted.placement.id,
        target: target(),
        old: conflicted.tip.unwrap(),
        placements: vec![conflicted.clone(), ordinary],
    };
    let plan = judge_reset(&reset, false).unwrap();
    assert_eq!(plan.skipped(), &[conflicted.placement.path.to_path_buf()]);
}

#[test]
fn apply_reset_preserves_conflicted_row_and_file_and_rechecks_skipped_rows() {
    use crate::placement::PlacementOps;
    let home = crate::test_support::TempHome::new().unwrap();
    let mut lock = home.home.try_lock().unwrap();
    let (mut registry, objects, refs, mut flagged) = conflict_fixture(&home.home, &mut lock);
    let fs = crate::placement_fs::PlacementFs::new(&home.home);
    flagged.conflict = crate::test_support::conflicted_facts().placement.conflict;
    let mut ordinary = flagged.clone();
    ordinary.path =
        crate::placement_path::PlacementPath::resolve(home.root.join("ordinary.txt")).unwrap();
    ordinary.conflict = None;
    ordinary.base = fs
        .create(
            ordinary.id,
            &ordinary.to_placement().1,
            &objects,
            hash_blob(b"ours\n"),
        )
        .unwrap()
        .base;
    registry
        .write(&lock, |tx| {
            tx.update_placement(&flagged)?;
            tx.insert_placement(&ordinary)
        })
        .unwrap();
    let stores = HistoryStores {
        objects: &objects,
        refs: &refs,
        object_limit: 16384,
    };
    let target = refs
        .read(RefKind::Branch, &RefName::parse("topic").unwrap())
        .unwrap()
        .unwrap();
    let facts = gather_reset(
        flagged.id,
        &RefName::parse("main").unwrap(),
        target,
        &fs,
        &stores,
        &registry,
    )
    .unwrap();
    let plan = judge_reset(&facts, false).unwrap();
    assert_eq!(plan.skipped(), &[flagged.path.to_path_buf()]);
    let mut changed = flagged.clone();
    changed.conflict.as_mut().unwrap().other = hash_blob(b"changed conflict");
    registry
        .write(&lock, |tx| tx.update_placement(&changed))
        .unwrap();
    assert!(matches!(
        apply_reset(&plan, &fs, &stores, &mut registry, &mut lock),
        Err(HistoryError::Refused(HistoryRefusal::PlacementsChanged))
    ));
    registry
        .write(&lock, |tx| tx.update_placement(&flagged))
        .unwrap();
    apply_reset(&plan, &fs, &stores, &mut registry, &mut lock).unwrap();
    assert_eq!(
        registry.placement_at(&flagged.path).unwrap(),
        Some(flagged.clone())
    );
    assert_eq!(
        std::fs::read(flagged.path.to_path_buf()).unwrap(),
        b"ours\n"
    );
    assert_eq!(
        registry
            .placement_at(&ordinary.path)
            .unwrap()
            .unwrap()
            .commit_id,
        Some(target)
    );
    assert_eq!(
        std::fs::read(ordinary.path.to_path_buf()).unwrap(),
        b"theirs\n"
    );
}

fn merge_metadata() -> CommitMetadata {
    CommitMetadata {
        author: crate::object::Author::anonymous(),
        time: crate::object::CommitTime::from_unix_seconds(0).unwrap(),
        message: "resolved merge".into(),
        body: None,
        group: None,
    }
}

fn with_conflicted_copy(
    test: impl FnOnce(
        &crate::home::Home,
        &mut crate::home::HomeLock,
        &mut Registry,
        &HistoryStores<'_>,
        &crate::placement_fs::PlacementFs<'_>,
        Placement,
    ),
) {
    use crate::placement::PlacementOps;
    let home = crate::test_support::TempHome::new().unwrap();
    let home = &home.home;
    let mut lock = home.try_lock().unwrap();
    let (mut registry, objects, refs, mut row) = conflict_fixture(home, &mut lock);
    let fs = crate::placement_fs::PlacementFs::new(home);
    let path = crate::placement_path::PlacementPath::resolve(home.path().join("copy.md")).unwrap();
    std::fs::rename(row.path.to_path_buf(), path.to_path_buf()).unwrap();
    let mut internal = row.clone();
    internal.kind = PlacementKind::Internal;
    internal.path = crate::placement_path::PlacementPath::resolve(fs.link_target(row.id)).unwrap();
    internal.base = fs
        .refresh(
            row.id,
            &internal.to_placement().1,
            &objects,
            hash_blob(b"ours\n"),
            None,
        )
        .unwrap()
        .base;
    registry
        .write(&lock, |tx| {
            tx.move_placement(&row.path, &path)?;
            tx.insert_placement(&internal)
        })
        .unwrap();
    row.path = path;
    let stores = HistoryStores {
        objects: &objects,
        refs: &refs,
        object_limit: 16384,
    };
    let outcome = home
        .merge_branch(
            &row,
            MergeRequest {
                other: &RefName::parse("topic").unwrap(),
                metadata: merge_metadata(),
            },
            &fs,
            &stores,
            &mut registry,
            &mut lock,
        )
        .unwrap();
    assert!(matches!(outcome, MergeOutcome::Conflicted(_)));
    let flagged = registry.placement_at(&row.path).unwrap().unwrap();
    assert_eq!(
        home.conflict_state(&row.path, &registry).unwrap(),
        flagged.conflict
    );
    test(home, &mut lock, &mut registry, &stores, &fs, flagged);
}

#[test]
fn abort_merge_restores_bytes_base_and_state_and_retains_pre_merge_blob() {
    use crate::placement::PlacementOps;
    with_conflicted_copy(|home, lock, registry, stores, fs, row| {
        let state = row.conflict.clone().unwrap();
        let main = RefName::parse("main").unwrap();
        let tip = stores.refs.read(RefKind::Branch, &main).unwrap();
        home.abort_merge(&row, fs, stores, registry, lock).unwrap();
        let restored = registry.placement_at(&row.path).unwrap().unwrap();
        assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), b"ours\n");
        assert_eq!(restored.conflict, None);
        assert_eq!(restored.staged, None);
        assert_eq!(restored.commit_id, row.commit_id);
        assert_eq!(restored.base.as_ref().unwrap().hash, hash_blob(b"ours\n"));
        assert_eq!(
            fs.hash_placement(&restored.to_placement().1).unwrap(),
            hash_blob(b"ours\n")
        );
        assert_eq!(stores.refs.read(RefKind::Branch, &main).unwrap(), tip);
        assert_eq!(
            stores
                .objects
                .get(state.pre_merge, ObjectKind::Blob, u64::MAX)
                .unwrap(),
            b"ours\n"
        );
    });
}

#[test]
fn continue_merge_keep_ours_records_resolution_and_two_parents() {
    for restage in [false, true] {
        with_conflicted_copy(|home, lock, registry, stores, fs, row| {
            let state = row.conflict.as_ref().unwrap();
            let bytes = stores
                .objects
                .get(state.pre_merge, ObjectKind::Blob, stores.object_limit)
                .unwrap();
            if restage {
                std::fs::write(row.path.to_path_buf(), b"resolved differently\n").unwrap();
                home.stage(&row, fs, stores.objects, stores.refs, registry, lock)
                    .unwrap();
                let staged = registry.placement_at(&row.path).unwrap().unwrap();
                assert_eq!(
                    staged.staged.unwrap().hash,
                    hash_blob(b"resolved differently\n")
                );
            }
            std::fs::write(row.path.to_path_buf(), &bytes).unwrap();
            let current = registry.placement_at(&row.path).unwrap().unwrap();
            home.stage(&current, fs, stores.objects, stores.refs, registry, lock)
                .unwrap();
            let staged = registry.placement_at(&row.path).unwrap().unwrap();
            assert_eq!(
                staged.staged,
                Some(StagedBlob {
                    hash: state.pre_merge,
                    size: bytes.len() as u64,
                }),
                "an explicitly staged keep-ours resolution must retain its stage"
            );
            assert_eq!(staged.base, row.base);
            assert_eq!(staged.commit_id, row.commit_id);
            assert_eq!(staged.conflict, row.conflict);
            let id = home
                .continue_merge(&staged, merge_metadata(), fs, stores, registry, lock)
                .unwrap();
            let commit = stores.objects.get_commit(id, stores.object_limit).unwrap();
            assert_eq!(commit.content, state.pre_merge);
            assert_eq!(commit.parents, vec![row.commit_id.unwrap(), state.other]);
            assert_eq!(
                stores
                    .refs
                    .read(RefKind::Branch, &RefName::parse("main").unwrap())
                    .unwrap(),
                Some(id)
            );
            let updated = registry.placement_at(&row.path).unwrap().unwrap();
            let internal = registry.internal_placement(row.id).unwrap().unwrap();
            for placement in [&updated, &internal] {
                assert_eq!(placement.commit_id, Some(id));
                assert_eq!(placement.base.as_ref().unwrap().hash, state.pre_merge);
                assert_eq!(placement.staged, None);
                assert_eq!(placement.conflict, None);
                assert_eq!(std::fs::read(placement.path.to_path_buf()).unwrap(), bytes);
            }
        });
    }
}

#[test]
fn continue_merge_keep_ours_requires_stage_after_restore_and_unstage() {
    with_conflicted_copy(|home, lock, registry, stores, fs, row| {
        let state = row.conflict.as_ref().unwrap();
        let bytes = stores
            .objects
            .get(state.pre_merge, ObjectKind::Blob, stores.object_limit)
            .unwrap();
        std::fs::write(row.path.to_path_buf(), &bytes).unwrap();
        for unstage in [false, true] {
            if unstage {
                home.stage(&row, fs, stores.objects, stores.refs, registry, lock)
                    .unwrap();
                let staged = registry.placement_at(&row.path).unwrap().unwrap();
                assert!(staged.staged.is_some());
                home.unstage(&staged, fs, registry, lock).unwrap();
            }
            let current = registry.placement_at(&row.path).unwrap().unwrap();
            assert_eq!(current.staged, None);
            assert_eq!(current.conflict, row.conflict);
            let error = home
                .continue_merge(&current, merge_metadata(), fs, stores, registry, lock)
                .unwrap_err();
            assert!(matches!(
                error,
                HistoryError::Refused(HistoryRefusal::NothingStaged)
            ));
            assert_eq!(registry.placement_at(&row.path).unwrap(), Some(current));
            assert_eq!(
                stores
                    .refs
                    .read(RefKind::Branch, &RefName::parse("main").unwrap())
                    .unwrap(),
                row.commit_id
            );
            assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), bytes);
        }
        home.stage(&row, fs, stores.objects, stores.refs, registry, lock)
            .unwrap();
        let staged = registry.placement_at(&row.path).unwrap().unwrap();
        home.continue_merge(&staged, merge_metadata(), fs, stores, registry, lock)
            .unwrap();
    });
}

#[test]
fn stage_equal_to_base_clears_stage_without_merge_conflict_state() {
    use crate::placement::ConflictOperation;
    for operation in [None, Some(ConflictOperation::Rebase)] {
        with_conflicted_copy(|home, lock, registry, stores, fs, mut row| {
            row.conflict = operation.map(|operation| {
                let mut state = row.conflict.clone().unwrap();
                state.operation = operation;
                state
            });
            registry
                .write(lock, |tx| tx.update_placement(&row))
                .unwrap();
            std::fs::write(row.path.to_path_buf(), b"different\n").unwrap();
            home.stage(&row, fs, stores.objects, stores.refs, registry, lock)
                .unwrap();
            let staged = registry.placement_at(&row.path).unwrap().unwrap();
            assert!(staged.staged.is_some());
            std::fs::write(row.path.to_path_buf(), b"ours\n").unwrap();
            home.stage(&staged, fs, stores.objects, stores.refs, registry, lock)
                .unwrap();
            let updated = registry.placement_at(&row.path).unwrap().unwrap();
            assert_eq!(updated.staged, None);
            assert_eq!(updated.base, row.base);
            assert_eq!(updated.conflict, row.conflict);
            assert_eq!(updated.commit_id, row.commit_id);
        });
    }
}

#[test]
fn continue_merge_records_staged_content_two_parents_and_clears_state() {
    with_conflicted_copy(|home, lock, registry, stores, fs, row| {
        let other = row.conflict.as_ref().unwrap().other;
        std::fs::write(row.path.to_path_buf(), b"resolved\n").unwrap();
        home.stage(&row, fs, stores.objects, stores.refs, registry, lock)
            .unwrap();
        let staged = registry.placement_at(&row.path).unwrap().unwrap();
        let content = staged.staged.unwrap();
        // The merge commit uses the stage even if the file is edited afterwards.
        std::fs::write(
            row.path.to_path_buf(),
            b"later edits\n<<<<<<<\n=======\n>>>>>>>\n",
        )
        .unwrap();
        let id = home
            .continue_merge(&staged, merge_metadata(), fs, stores, registry, lock)
            .unwrap();
        let commit = stores.objects.get_commit(id, stores.object_limit).unwrap();
        assert_eq!(commit.parents, vec![row.commit_id.unwrap(), other]);
        assert_eq!(commit.content, content.hash);
        assert_eq!(
            stores
                .objects
                .get(commit.content, ObjectKind::Blob, u64::MAX)
                .unwrap(),
            b"resolved\n"
        );
        assert_eq!(
            stores
                .refs
                .read(RefKind::Branch, &RefName::parse("main").unwrap())
                .unwrap(),
            Some(id)
        );
        let updated = registry.placement_at(&row.path).unwrap().unwrap();
        assert_eq!(updated.commit_id, Some(id));
        assert_eq!(updated.conflict, None);
        assert_eq!(updated.staged, None);
        assert_eq!(home.conflict_state(&row.path, registry).unwrap(), None);
        assert_eq!(
            std::fs::read(row.path.to_path_buf()).unwrap(),
            b"later edits\n<<<<<<<\n=======\n>>>>>>>\n"
        );
        let internal = registry.internal_placement(row.id).unwrap().unwrap();
        assert_eq!(internal.commit_id, Some(id));
        assert_eq!(
            std::fs::read(internal.path.to_path_buf()).unwrap(),
            b"resolved\n"
        );
    });
}

#[test]
fn continue_merge_refuses_moved_branch_like_commit_and_keeps_state() {
    with_conflicted_copy(|home, lock, registry, stores, fs, row| {
        std::fs::write(row.path.to_path_buf(), b"resolved\n").unwrap();
        home.stage(&row, fs, stores.objects, stores.refs, registry, lock)
            .unwrap();
        let staged = registry.placement_at(&row.path).unwrap().unwrap();
        let main = RefName::parse("main").unwrap();
        let other = row.conflict.as_ref().unwrap().other;
        stores
            .refs
            .update(
                lock,
                RefKind::Branch,
                &main,
                other,
                crate::history::Expect::Is(row.commit_id.unwrap()),
            )
            .unwrap();
        let mut ordinary = gather_commit(&staged, fs, stores, registry).unwrap();
        ordinary.working.placement.conflict = None;
        let expected = judge_commit(&ordinary).unwrap_err();
        assert!(matches!(expected, HistoryRefusal::BranchMoved { .. }));
        let error = home
            .continue_merge(&staged, merge_metadata(), fs, stores, registry, lock)
            .unwrap_err();
        assert_eq!(error.to_string(), expected.to_string());
        assert!(matches!(
            error,
            HistoryError::Refused(HistoryRefusal::BranchMoved { .. })
        ));
        assert_eq!(registry.placement_at(&row.path).unwrap(), Some(staged));
        assert_eq!(
            stores.refs.read(RefKind::Branch, &main).unwrap(),
            Some(other)
        );
        let commit = stores
            .objects
            .get_commit(other, stores.object_limit)
            .unwrap();
        assert_eq!(commit.parents.len(), 1);
    });
}

#[test]
fn abort_and_continue_refuse_unflagged_copy_without_changes() {
    with_conflicted_copy(|home, lock, registry, stores, fs, row| {
        home.abort_merge(&row, fs, stores, registry, lock).unwrap();
        let row = registry.placement_at(&row.path).unwrap().unwrap();
        let bytes = std::fs::read(row.path.to_path_buf()).unwrap();
        let errors = [
            home.abort_merge(&row, fs, stores, registry, lock)
                .unwrap_err(),
            home.continue_merge(&row, merge_metadata(), fs, stores, registry, lock)
                .unwrap_err(),
        ];
        for error in errors {
            assert_eq!(
                error.to_string(),
                format!(
                    "copy has no merge to abort or continue: {}",
                    row.path.to_path_buf().display()
                )
            );
            assert!(
                matches!(error, HistoryError::Refused(HistoryRefusal::NotConflicted(ref path)) if *path == row.path.to_path_buf())
            );
        }
        assert_eq!(registry.placement_at(&row.path).unwrap(), Some(row.clone()));
        assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), bytes);
        assert_eq!(
            stores
                .refs
                .read(RefKind::Branch, &RefName::parse("main").unwrap())
                .unwrap(),
            row.commit_id
        );
    });
}

#[test]
fn abort_merge_clears_stage_and_rebuilds_references_from_restored_bytes() {
    with_conflicted_copy(|home, lock, registry, stores, fs, row| {
        let resolved = b"---\nfolio-references:\n  child: {name: missing-child}\n---\nresolved\n";
        std::fs::write(row.path.to_path_buf(), resolved).unwrap();
        home.stage(&row, fs, stores.objects, stores.refs, registry, lock)
            .unwrap();
        let staged = registry.placement_at(&row.path).unwrap().unwrap();
        assert!(staged.staged.is_some());
        assert_eq!(registry.references(row.id, &row.head).unwrap().len(), 1);
        home.abort_merge(&staged, fs, stores, registry, lock)
            .unwrap();
        let restored = registry.placement_at(&row.path).unwrap().unwrap();
        assert_eq!(restored.staged, None);
        assert_eq!(restored.conflict, None);
        assert!(registry.references(row.id, &row.head).unwrap().is_empty());
        assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), b"ours\n");
        assert_eq!(restored.commit_id, row.commit_id);
    });
}

#[test]
fn continue_merge_requires_stage_and_stage_refuses_recorded_markers() {
    with_conflicted_copy(|home, lock, registry, stores, fs, row| {
        let error = home
            .stage(&row, fs, stores.objects, stores.refs, registry, lock)
            .unwrap_err();
        assert!(matches!(
            error,
            crate::ops::rules_error::RulesError::Refused(crate::ops::refusal::PlacementRefusal {
                reason: crate::ops::refusal::RefusalReason::ConflictMarkers,
                ..
            })
        ));
        assert_eq!(registry.placement_at(&row.path).unwrap(), Some(row.clone()));
        assert_eq!(
            stores
                .refs
                .read(RefKind::Branch, &RefName::parse("main").unwrap())
                .unwrap(),
            row.commit_id
        );
        std::fs::write(row.path.to_path_buf(), b"resolved\n").unwrap();
        let error = home
            .continue_merge(&row, merge_metadata(), fs, stores, registry, lock)
            .unwrap_err();
        assert!(matches!(
            error,
            HistoryError::Refused(HistoryRefusal::NothingStaged)
        ));
        assert_eq!(registry.placement_at(&row.path).unwrap(), Some(row));
    });
}

#[test]
fn continue_merge_refuses_persisted_staged_conflict_lines() {
    use crate::ops::refusal::{PlacementRefusal, RefusalReason};
    let mut large = vec![b'x'; 20000];
    large.extend_from_slice(b"\n=======\n");
    for bytes in [
        large.as_slice(),
        b"<<<<<<<\nours\n=======\ntheirs\n>>>>>>>\n".as_slice(),
        b"resolved first\nseparator\n<<<<<<<\nours second\n=======\ntheirs second\n>>>>>>>\n",
        b"<<<<<<<\nours first\n=======\ntheirs first\n>>>>>>>\nseparator\nresolved second\n",
        b"Heading\n=======\n",
    ] {
        with_conflicted_copy(|home, lock, registry, stores, fs, mut row| {
            row.conflict.as_mut().unwrap().marker_lines.extend([
                b"<<<<<<<\n".to_vec(),
                b"=======\n".to_vec(),
                b">>>>>>>\n".to_vec(),
            ]);
            let hash = stores.objects.put(lock, ObjectKind::Blob, bytes).unwrap();
            row.staged = Some(StagedBlob {
                hash,
                size: bytes.len() as u64,
            });
            registry
                .write(lock, |tx| tx.update_placement(&row))
                .unwrap();
            std::fs::write(row.path.to_path_buf(), b"resolved\n").unwrap();
            let mut reopened = Registry::open_existing(home).unwrap();
            let row = reopened.placement_at(&row.path).unwrap().unwrap();
            let placements = reopened.placements_of_head(row.id, &row.head).unwrap();
            let references = reopened.references(row.id, &row.head).unwrap();
            let before = std::fs::read_dir(
                home.path()
                    .join("folios")
                    .join(row.id.to_string())
                    .join("blobs"),
            )
            .unwrap()
            .count();
            assert!(matches!(
                home.continue_merge(&row, merge_metadata(), fs, stores, &mut reopened, lock),
                Err(HistoryError::Refused(HistoryRefusal::Move(
                    PlacementRefusal {
                        reason: RefusalReason::ConflictMarkers,
                        ..
                    }
                )))
            ));
            assert_eq!(
                reopened.placements_of_head(row.id, &row.head).unwrap(),
                placements
            );
            assert_eq!(reopened.references(row.id, &row.head).unwrap(), references);
            assert_eq!(
                std::fs::read(row.path.to_path_buf()).unwrap(),
                b"resolved\n"
            );
            assert_eq!(
                std::fs::read_dir(
                    home.path()
                        .join("folios")
                        .join(row.id.to_string())
                        .join("blobs")
                )
                .unwrap()
                .count(),
                before
            );
            assert_eq!(
                stores
                    .refs
                    .read(RefKind::Branch, &RefName::parse(&row.head).unwrap())
                    .unwrap(),
                row.commit_id
            );
        });
    }
}
