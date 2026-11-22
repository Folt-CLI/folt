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
