use super::*;

#[test]
fn unrelated_published_commit_preserves_conflict_and_rows() {
    for changed in [
        "content",
        "reversed parents",
        "other parent",
        "target parent",
        "author",
        "message",
        "body",
        "group",
    ] {
        with_conflicted_copy(|home, lock, registry, stores, fs, row| {
            let row = stage_resolution(home, lock, registry, stores, fs, &row);
            assert!(
                continue_steps(
                    &row,
                    merge_metadata(),
                    fs,
                    stores,
                    registry,
                    lock,
                    fail_at(ContinueStep::BranchPublished)
                )
                .is_err()
            );
            let branch = RefName::parse(&row.head).unwrap();
            let published = stores.refs.read(RefKind::Branch, &branch).unwrap().unwrap();
            let mut commit = stores
                .objects
                .get_commit(published, stores.object_limit)
                .unwrap();
            match changed {
                "content" => {
                    commit.content = stores
                        .objects
                        .put(lock, ObjectKind::Blob, b"different\n")
                        .unwrap()
                }
                "reversed parents" => commit.parents.reverse(),
                "other parent" => commit.parents[1] = commit.parents[0],
                "target parent" => commit.parents[0] = commit.parents[1],
                "author" => commit.author = crate::object::Author::new("other", None).unwrap(),
                "message" => commit.message = "other".into(),
                "body" => commit.body = Some("other".into()),
                "group" => commit.group = Some("other".into()),
                _ => unreachable!(),
            }
            let unrelated = stores.objects.put_commit(lock, &commit).unwrap();
            stores
                .refs
                .update(
                    lock,
                    RefKind::Branch,
                    &branch,
                    unrelated,
                    crate::history::Expect::Is(published),
                )
                .unwrap();
            let mut reopened = Registry::open_or_create(home, lock).unwrap();
            let before = reopened.placements_of_head(row.id, &row.head).unwrap();
            assert!(
                matches!(
                    continue_merge(&row, merge_metadata(), fs, stores, &mut reopened, lock),
                    Err(HistoryError::Refused(HistoryRefusal::BranchMoved { .. }))
                ),
                "{changed}"
            );
            assert_eq!(
                reopened.placements_of_head(row.id, &row.head).unwrap(),
                before
            );
            assert_eq!(
                stores.refs.read(RefKind::Branch, &branch).unwrap(),
                Some(unrelated)
            );
            assert_eq!(
                std::fs::read(row.path.to_path_buf()).unwrap(),
                b"resolved\n"
            );
            let internal = reopened.internal_placement(row.id).unwrap().unwrap();
            assert_eq!(
                std::fs::read(internal.path.to_path_buf()).unwrap(),
                b"ours\n"
            );
        });
    }
}

#[test]
fn retry_refuses_missing_stage_and_unrelated_internal_edits() {
    with_conflicted_copy(|home, lock, registry, stores, fs, row| {
        let row = stage_resolution(home, lock, registry, stores, fs, &row);
        assert!(
            continue_steps(
                &row,
                merge_metadata(),
                fs,
                stores,
                registry,
                lock,
                fail_at(ContinueStep::InternalMoved)
            )
            .is_err()
        );
        let internal = registry.internal_placement(row.id).unwrap().unwrap();
        std::fs::write(internal.path.to_path_buf(), b"unrelated edits\n").unwrap();
        let before = registry.placements_of_head(row.id, &row.head).unwrap();
        assert!(matches!(
            continue_merge(&row, merge_metadata(), fs, stores, registry, lock),
            Err(HistoryError::Refused(HistoryRefusal::InternalEdited(_)))
        ));
        assert_eq!(
            registry.placements_of_head(row.id, &row.head).unwrap(),
            before
        );
        let mut unstaged = row.clone();
        unstaged.staged = None;
        registry
            .write(lock, |tx| tx.update_placement(&unstaged))
            .unwrap();
        assert!(matches!(
            continue_merge(&unstaged, merge_metadata(), fs, stores, registry, lock),
            Err(HistoryError::Refused(HistoryRefusal::NothingStaged))
        ));
        assert_eq!(registry.placement_at(&row.path).unwrap(), Some(unstaged));
        assert_eq!(
            std::fs::read(internal.path.to_path_buf()).unwrap(),
            b"unrelated edits\n"
        );
    });
}
