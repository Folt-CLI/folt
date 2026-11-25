use super::*;
use crate::object::hash_blob;
use crate::ops::history::{MergeOutcome, MergeRequest};

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
    let (mut registry, objects, refs, mut row) =
        crate::ops::history::tests::conflict_fixture(home, &mut lock);
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

fn fail_at(target: ContinueStep) -> impl FnMut(ContinueStep) -> Result<(), HistoryError> {
    move |step| {
        if step == target {
            Err(HistoryRefusal::InvalidFacts.into())
        } else {
            Ok(())
        }
    }
}

fn stage_resolution(
    home: &crate::home::Home,
    lock: &HomeLock,
    registry: &mut Registry,
    stores: &HistoryStores<'_>,
    fs: &impl PlacementOps,
    row: &Placement,
) -> Placement {
    std::fs::write(row.path.to_path_buf(), b"resolved\n").unwrap();
    home.stage(row, fs, stores.objects, stores.refs, registry, lock)
        .unwrap();
    registry.placement_at(&row.path).unwrap().unwrap()
}

#[test]
fn published_merge_retry_completes_persisted_rows() {
    for step in [ContinueStep::BranchPublished, ContinueStep::InternalMoved] {
        for later_edits in [false, true] {
            with_conflicted_copy(|home, lock, registry, stores, fs, row| {
                let row = stage_resolution(home, lock, registry, stores, fs, &row);
                let before = registry.placements_of_head(row.id, &row.head).unwrap();
                assert_eq!(before.len(), 2);
                assert!(
                    continue_steps(
                        &row,
                        merge_metadata(),
                        fs,
                        stores,
                        registry,
                        lock,
                        fail_at(step)
                    )
                    .is_err()
                );
                let branch = RefName::parse(&row.head).unwrap();
                let published = stores.refs.read(RefKind::Branch, &branch).unwrap().unwrap();
                let commit = stores
                    .objects
                    .get_commit(published, stores.object_limit)
                    .unwrap();
                assert_eq!(
                    commit.parents,
                    vec![row.commit_id.unwrap(), row.conflict.as_ref().unwrap().other]
                );
                assert_eq!(commit.content, row.staged.unwrap().hash);
                assert_eq!(
                    registry.placements_of_head(row.id, &row.head).unwrap(),
                    before
                );
                let internal = registry.internal_placement(row.id).unwrap().unwrap();
                assert_eq!(
                    std::fs::read(internal.path.to_path_buf()).unwrap(),
                    if step == ContinueStep::InternalMoved {
                        b"resolved\n".as_slice()
                    } else {
                        b"ours\n".as_slice()
                    }
                );
                if later_edits {
                    std::fs::write(row.path.to_path_buf(), b"later edits\n").unwrap();
                }
                let mut reopened = Registry::open_or_create(home, lock).unwrap();
                let row = reopened.placement_at(&row.path).unwrap().unwrap();
                let mut metadata = merge_metadata();
                metadata.time = crate::object::CommitTime::from_unix_seconds(42).unwrap();
                metadata.body = Some(String::new());
                let result = continue_merge(&row, metadata, fs, stores, &mut reopened, lock);
                assert!(result.is_ok(), "retry failed: {result:?}");
                assert_eq!(result.unwrap(), published);
                assert_eq!(
                    stores
                        .objects
                        .get_commit(published, stores.object_limit)
                        .unwrap(),
                    commit
                );
                assert_eq!(
                    stores.refs.read(RefKind::Branch, &branch).unwrap(),
                    Some(published)
                );
                for updated in reopened.placements_of_head(row.id, &row.head).unwrap() {
                    assert_eq!(updated.commit_id, Some(published));
                    assert_eq!(updated.staged, None);
                    assert_eq!(updated.conflict, None);
                    assert_eq!(updated.base.unwrap().hash, commit.content);
                }
                assert_eq!(
                    std::fs::read(row.path.to_path_buf()).unwrap(),
                    if later_edits {
                        b"later edits\n".as_slice()
                    } else {
                        b"resolved\n".as_slice()
                    }
                );
                assert_eq!(
                    std::fs::read(internal.path.to_path_buf()).unwrap(),
                    b"resolved\n"
                );
            });
        }
    }
}

#[test]
fn ordinary_one_parent_recovery_still_reuses_published_commit() {
    with_conflicted_copy(|home, lock, registry, stores, fs, row| {
        home.abort_merge(&row, fs, stores, registry, lock).unwrap();
        let row = registry.placement_at(&row.path).unwrap().unwrap();
        let row = stage_resolution(home, lock, registry, stores, fs, &row);
        let commit = build_commit(
            row.staged.unwrap().hash,
            row.commit_id.into_iter().collect(),
            merge_metadata(),
        )
        .unwrap();
        let published = stores.objects.put_commit(lock, &commit).unwrap();
        stores
            .refs
            .update(
                lock,
                RefKind::Branch,
                &RefName::parse(&row.head).unwrap(),
                published,
                crate::history::Expect::Is(row.commit_id.unwrap()),
            )
            .unwrap();
        let facts = home.gather_commit(&row, fs, stores, registry).unwrap();
        let plan = home.judge_commit(&facts).unwrap();
        assert_eq!(plan.resumed_commit(), Some(published));
        let mut metadata = merge_metadata();
        metadata.time = crate::object::CommitTime::from_unix_seconds(42).unwrap();
        assert_eq!(
            home.apply_commit(&plan, metadata, fs, stores, registry, lock)
                .unwrap(),
            published
        );
        assert_eq!(
            stores
                .objects
                .get_commit(published, stores.object_limit)
                .unwrap(),
            commit
        );
        for updated in registry.placements_of_head(row.id, &row.head).unwrap() {
            assert_eq!(updated.commit_id, Some(published));
            assert_eq!(updated.staged, None);
        }
    });
}

mod refusals;
