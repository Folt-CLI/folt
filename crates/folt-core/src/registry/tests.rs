use super::*;
use crate::test_support::TempDir;

#[test]
fn schema_three_indexes_names_and_rejects_invalid_stored_names() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("home");
    let home = Home::open_or_create(path.clone()).unwrap();
    let lock = home.try_lock().unwrap();
    let registry = Registry::open_or_create(&home, &lock).unwrap();
    let connection = registry.connection.as_ref().unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 3);
    let plan: String = connection
        .query_row(
            "EXPLAIN QUERY PLAN SELECT id FROM folios WHERE name=?1",
            ["logo"],
            |r| r.get(3),
        )
        .unwrap();
    assert!(
        plan.contains("SEARCH") && plan.contains("folios_name"),
        "{plan}"
    );
    let plan: String = connection
        .query_row(
            "EXPLAIN QUERY PLAN SELECT parent FROM refs WHERE child=?1",
            ["target"],
            |r| r.get(3),
        )
        .unwrap();
    assert!(
        plan.contains("SEARCH") && plan.contains("refs_child"),
        "{plan}"
    );
    let id = FolioId::generate();
    connection
        .execute(
            "INSERT INTO folios(id,layout,algorithm,name) VALUES (?1,'1','sha256','bad/name')",
            [id.as_bytes().as_slice()],
        )
        .unwrap();
    assert!(
        matches!(registry.folio_name(id), Err(RegistryError::InvalidName(text)) if text == "bad/name")
    );
    drop(registry);
    drop(lock);
}

#[test]
fn placement_writers_report_outcomes_without_partial_prefix_moves() {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE TABLE placements(path BLOB PRIMARY KEY);")
        .unwrap();
    let key = |text: &str| {
        PlacementPath(path_bytes(Path::new(
            &text.replace('/', std::path::MAIN_SEPARATOR_STR),
        )))
    };
    let prefix = |text: &str| {
        PlacementPrefix(path_bytes(Path::new(
            &text.replace('/', std::path::MAIN_SEPARATOR_STR),
        )))
    };
    let a = key("/old/a");
    let b = key("/old/b");
    let collision = key("/new/b");
    for path in [&a, &b, &collision] {
        connection
            .execute("INSERT INTO placements(path) VALUES (?1)", [&path.0])
            .unwrap();
    }
    // These writers only need the path column; full row reads are not needed.
    let tx = connection.transaction().unwrap();
    let mut writer = RegistryTransaction { tx };
    assert_eq!(
        writer.remove_placement(&key("/missing")).unwrap(),
        RemoveResult::NotRegistered
    );
    assert_eq!(
        writer
            .move_prefix(&prefix("/old"), &prefix("/old/child"))
            .unwrap(),
        MoveResult::InsideSource
    );
    assert_eq!(
        writer
            .move_prefix(&prefix("/missing"), &prefix("/new"))
            .unwrap(),
        MoveResult::NotRegistered
    );
    assert_eq!(
        writer
            .move_prefix(&prefix("/old"), &prefix("/new"))
            .unwrap(),
        MoveResult::DestinationRegistered
    );
    assert_eq!(
        writer
            .tx
            .query_row(
                "SELECT count(*) FROM placements WHERE path=?1",
                [&a.0],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        writer.move_placement(&key("/missing"), &a).unwrap(),
        MoveResult::NotRegistered
    );
    assert_eq!(
        writer.move_placement(&a, &b).unwrap(),
        MoveResult::DestinationRegistered
    );
    assert_eq!(
        writer
            .move_placement(&collision, &key("/elsewhere"))
            .unwrap(),
        MoveResult::Moved
    );
    assert_eq!(
        writer.remove_placement(&key("/elsewhere")).unwrap(),
        RemoveResult::Removed
    );
    assert_eq!(
        writer
            .move_prefix(&prefix("/old"), &prefix("/new"))
            .unwrap(),
        MoveResult::Moved
    );
}
