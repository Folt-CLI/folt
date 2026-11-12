use super::*;
use crate::config::env::Environment;
use crate::placement::StagedBlob;
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
    assert!(matches!(judge_commit(&facts), Err(Refusal::Link(_))));
    facts.working.placement.kind = PlacementKind::Copy;
    facts.working.placement.staged = None;
    assert_eq!(judge_commit(&facts).unwrap_err(), Refusal::NothingStaged);
    let mut facts = commit_facts();
    facts.working.observation = Observation::Missing;
    assert!(matches!(judge_commit(&facts), Err(Refusal::Unusable(_))));
}

#[test]
fn commit_refuses_moved_branch_and_invalid_internal_head() {
    let mut facts = commit_facts();
    facts.working.tip = Some(hash_blob(b"advanced"));
    assert!(matches!(
        judge_commit(&facts),
        Err(Refusal::BranchMoved { .. })
    ));
    facts.working.placement.kind = PlacementKind::Internal;
    assert_eq!(judge_commit(&facts).unwrap_err(), Refusal::InvalidFacts);
}

#[test]
fn main_copy_requires_clean_internal_file() {
    let mut facts = commit_facts();
    facts.working.placement.head = "main".into();
    assert_eq!(judge_commit(&facts).unwrap_err(), Refusal::MissingInternal);
    let mut internal = facts.working.clone();
    internal.placement.kind = PlacementKind::Internal;
    internal.placement_hash = Some(hash_blob(b"edited"));
    facts.internal = Some(internal);
    assert!(matches!(
        judge_commit(&facts),
        Err(Refusal::InternalEdited(_))
    ));
    facts.internal.as_mut().unwrap().placement.id = FolioId::from_bytes([2; 16]);
    assert_eq!(judge_commit(&facts).unwrap_err(), Refusal::InvalidFacts);
}

#[test]
fn main_copy_refuses_clean_internal_with_stage() {
    let mut facts = commit_facts();
    facts.working.placement.head = "main".into();
    let mut internal = facts.working.clone();
    internal.placement.kind = PlacementKind::Internal;
    assert_eq!(working_state::judge_working(&internal).edit, Edit::Clean);
    facts.internal = Some(internal);
    let refusal = judge_commit(&facts).unwrap_err();
    assert!(matches!(refusal, Refusal::InternalEdited(_)));
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
    assert!(matches!(judge_reset(&reset, false), Err(Refusal::Move(_))));
    assert!(judge_reset(&reset, true).is_ok());
}

#[test]
fn branch_sync_failure_leaves_cached_rows_untouched_and_harmless() {
    use crate::config::formats::OBJECT_ALGORITHM;
    use crate::folio_dir::{Durability, FolioSpec, create_folio};
    use crate::home::Home;
    use crate::layout::FOLIO_LAYOUT_VERSION;
    use crate::registry::{FolioFormat, NewFolio};

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
        let refs = RefStore::open(&folio, &format).unwrap();
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
        let result = crate::fs::with_sync_failure(&folio.path().join("refs"), || {
            delete_branch(folio.id(), &topic, old, &refs, &registry, &mut lock)
        });
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("injected directory sync failure")
        );
        assert_eq!(refs.read(Kind::Branch, &topic).unwrap(), None);
        assert_eq!(registry.references(folio.id(), "topic").unwrap().len(), 1);
        registry
            .write(&lock, |tx| {
                tx.remove_folio(target, &mut |parent, branch| {
                    assert_eq!(parent, folio.id());
                    Ok(refs
                        .read(Kind::Branch, &RefName::parse(branch).unwrap())
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
        Refusal::BranchInUse(vec![row.path.to_path_buf()])
    );
    row.kind = PlacementKind::Internal;
    row.head = "main".into();
    assert!(matches!(
        judge_delete_branch(&[row]),
        Err(Refusal::BranchInUse(_))
    ));
}
