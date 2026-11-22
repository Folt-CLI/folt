use std::path::Path;

use rusqlite::Connection;

use super::{MoveResult, Registry, RegistryTransaction, RemoveResult};
use crate::config::env::Environment;
use crate::home::Home;
use crate::id::FolioId;
use crate::placement_path::{PlacementPath, PlacementPrefix, RegistryError, path_bytes};
use crate::test_support::TempDir;

#[test]
fn schema_three_indexes_names_and_rejects_invalid_stored_names() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("home");
    let home = Home::open_or_create(path.clone(), &Environment::default()).unwrap();
    let lock = home.try_lock().unwrap();
    let registry = Registry::open_or_create(&home, &lock).unwrap();
    let connection = registry.connection.as_ref().unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 4);
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

#[test]
fn fresh_writer_creates_a_current_schema_that_both_paths_reopen() {
    use super::schema::{SchemaState, classify_schema};
    use crate::config::formats::APPLICATION_ID;
    use crate::config::names::REGISTRY_FILE;

    let directory = TempDir::new().unwrap();
    let home =
        Home::open_or_create(directory.path().join("home"), &Environment::default()).unwrap();
    let lock = home.try_lock().unwrap();
    let path = home.path().join(REGISTRY_FILE);
    let registry = Registry::open_or_create(&home, &lock).unwrap();
    let connection = registry.connection.as_ref().unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let app: i64 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 4);
    assert_eq!(app, i64::from(APPLICATION_ID));
    assert!(matches!(
        classify_schema(version, app, &path).unwrap(),
        SchemaState::Current
    ));
    drop(registry);
    assert!(Registry::open_or_create(&home, &lock).is_ok());
    assert!(Registry::open_existing(&home).is_ok());
}

#[test]
fn schema_version_and_application_id_preserve_both_open_paths() {
    use crate::config::formats::APPLICATION_ID;
    use crate::config::names::REGISTRY_FILE;

    let folt = i64::from(APPLICATION_ID);
    // Expected outcomes are writer, then read-only. A zero/zero database is
    // prepared explicitly here; the separate fresh-file test covers creation.
    let cases = [
        (0, 0, "ok", "foreign"),
        (0, 42, "foreign", "foreign"),
        (0, folt, "foreign", "foreign"),
        (1, 0, "foreign", "foreign"),
        (1, 42, "foreign", "foreign"),
        (1, folt, "outdated", "outdated"),
        (2, 0, "foreign", "foreign"),
        (2, 42, "foreign", "foreign"),
        (2, folt, "ok", "foreign"),
        (3, 0, "foreign", "foreign"),
        (3, 42, "foreign", "foreign"),
        (3, folt, "ok", "foreign"),
        (4, folt, "ok", "ok"),
        (4, 0, "foreign", "foreign"),
        (4, 42, "foreign", "foreign"),
        (5, 0, "unsupported", "unsupported"),
        (5, 42, "unsupported", "unsupported"),
        (5, folt, "unsupported", "unsupported"),
    ];
    for (version, app, writer_expected, reader_expected) in cases {
        for (writer, expected) in [(true, writer_expected), (false, reader_expected)] {
            let directory = TempDir::new().unwrap();
            let home = Home::open_or_create(directory.path().join("home"), &Environment::default())
                .unwrap();
            let lock = home.try_lock().unwrap();
            let path = home.path().join(REGISTRY_FILE);
            let connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "user_version", version)
                .unwrap();
            connection
                .pragma_update(None, "application_id", app)
                .unwrap();
            drop(connection);
            let result = if writer {
                Registry::open_or_create(&home, &lock)
            } else {
                Registry::open_existing(&home)
            };
            let context = format!("version={version}, app={app}, writer={writer}");
            match (expected, result) {
                ("ok", Ok(_)) => {}
                ("foreign", Err(error @ RegistryError::ForeignDatabase(_))) => {
                    assert!(
                        matches!(&error, RegistryError::ForeignDatabase(found) if found == &path),
                        "{context}"
                    );
                    assert_eq!(
                        error.to_string(),
                        format!("not a Folt registry database: {}", path.display()),
                        "{context}"
                    );
                }
                ("outdated", Err(error @ RegistryError::OutdatedSchema { .. })) => {
                    assert!(
                        matches!(&error, RegistryError::OutdatedSchema { found: 1, required: 4, path: found } if found == &path),
                        "{context}"
                    );
                    assert_eq!(
                        error.to_string(),
                        format!(
                            "registry {} uses pre-release schema version 1; required version is 4; delete this registry before continuing",
                            path.display()
                        ),
                        "{context}"
                    );
                }
                ("unsupported", Err(error @ RegistryError::UnsupportedSchemaVersion { .. })) => {
                    assert!(
                        matches!(
                            error,
                            RegistryError::UnsupportedSchemaVersion {
                                found: 5,
                                supported: 4
                            }
                        ),
                        "{context}"
                    );
                    assert_eq!(
                        error.to_string(),
                        "registry schema version 5 is newer than supported version 4",
                        "{context}"
                    );
                }
                (_, result) => panic!("{context}: expected {expected}, got {:?}", result.err()),
            }
        }
    }
}

#[test]
fn missing_read_only_registry_is_an_empty_view() {
    let directory = TempDir::new().unwrap();
    let home =
        Home::open_or_create(directory.path().join("home"), &Environment::default()).unwrap();
    let registry = Registry::open_existing(&home).unwrap();
    assert!(registry.connection.is_none());
    assert!(!registry.path.exists());
}

fn open_registry(directory: &TempDir) -> (Home, crate::home::HomeLock) {
    let home =
        Home::open_or_create(directory.path().join("home"), &Environment::default()).unwrap();
    let lock = home.try_lock().unwrap();
    (home, lock)
}

#[test]
fn backup_writes_a_snapshot_and_leaves_no_temporary_file() {
    let directory = TempDir::new().unwrap();
    let (home, lock) = open_registry(&directory);
    let registry = Registry::open_or_create(&home, &lock).unwrap();
    let backups = directory.path().join("backups");
    std::fs::create_dir(&backups).unwrap();
    registry
        .backup_to(&lock, backups.join("registry.db"))
        .unwrap();
    let names: Vec<_> = std::fs::read_dir(&backups)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("registry.db")]);
}

#[test]
fn failed_backup_publication_leaves_no_temporary_file() {
    let directory = TempDir::new().unwrap();
    let (home, lock) = open_registry(&directory);
    let registry = Registry::open_or_create(&home, &lock).unwrap();
    let backups = directory.path().join("backups");
    std::fs::create_dir(&backups).unwrap();
    // A directory at the destination makes the final rename fail after the snapshot is written.
    let destination = backups.join("registry.db");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("keep"), b"x").unwrap();
    assert!(registry.backup_to(&lock, &destination).is_err());
    let names: Vec<_> = std::fs::read_dir(&backups)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("registry.db")]);
}

#[test]
fn conflict_state_round_trip_and_copy_only_checks() {
    use crate::object::hash_blob;
    use crate::placement::{ConflictOperation, ConflictState, PlacementKind};
    let directory = TempDir::new().unwrap();
    let (home, lock) = open_registry(&directory);
    let mut registry = Registry::open_or_create(&home, &lock).unwrap();
    let mut row = crate::test_support::facts().placement;
    row.conflict = Some(ConflictState {
        operation: ConflictOperation::Merge,
        other: hash_blob(b"other"),
        marker_lines: vec![
            b"<<<<<<<\n".to_vec(),
            vec![],
            b"=======\n".to_vec(),
            b">>>>>>>\n".to_vec(),
        ],
        pre_merge: hash_blob(b"before"),
    });
    registry
        .write(&lock, |tx| {
            tx.add_folio(&crate::folio::row::NewFolio {
                id: row.id,
                format: crate::folio::format::FolioFormat {
                    layout: crate::layout::FOLIO_LAYOUT_VERSION,
                    algorithm: crate::config::formats::OBJECT_ALGORITHM.into(),
                },
            })?;
            tx.insert_placement(&row)
        })
        .unwrap();
    assert_eq!(registry.placement_at(&row.path).unwrap(), Some(row.clone()));
    assert_eq!(registry.all_placements().unwrap(), vec![row.clone()]);
    assert_eq!(registry.placements_of(row.id).unwrap(), vec![row.clone()]);
    assert_eq!(
        registry.placements_of_head(row.id, &row.head).unwrap(),
        vec![row.clone()]
    );
    registry
        .write(&lock, |tx| {
            assert_eq!(tx.placement_at(&row.path)?, Some(row.clone()));
            Ok(())
        })
        .unwrap();
    drop(registry);
    let mut registry = Registry::open_or_create(&home, &lock).unwrap();
    assert_eq!(registry.placement_at(&row.path).unwrap(), Some(row.clone()));
    row.conflict.as_mut().unwrap().marker_lines = vec![b"changed\0\n".to_vec()];
    registry
        .write(&lock, |tx| tx.update_placement(&row))
        .unwrap();
    assert_eq!(registry.placement_at(&row.path).unwrap(), Some(row.clone()));
    for kind in [PlacementKind::Internal, PlacementKind::Link] {
        let mut invalid = row.clone();
        invalid.kind = kind;
        invalid.head = "main".into();
        invalid.base = None;
        invalid.commit_id = None;
        assert!(
            registry
                .write(&lock, |tx| tx.update_placement(&invalid))
                .is_err()
        );
        assert_eq!(registry.placement_at(&row.path).unwrap(), Some(row.clone()));
    }
    let connection = registry.connection.as_ref().unwrap();
    assert!(
        connection
            .execute("UPDATE placements SET conflict_other=NULL", [])
            .is_err()
    );
    connection
        .execute("UPDATE placements SET conflict_markers=?1", [vec![0u8; 7]])
        .unwrap();
    assert!(registry.placement_at(&row.path).is_err());
}

#[test]
fn malformed_conflict_encodings_return_registry_errors() {
    use super::codec::decode_conflict;
    let id = || Some(vec![0; 32]);
    for markers in [
        vec![0; 7],
        u64::MAX.to_be_bytes().to_vec(),
        [1u64.to_be_bytes().as_slice()].concat(),
    ] {
        assert!(matches!(
            decode_conflict(Some("merge".into()), id(), Some(markers), id()),
            Err(RegistryError::InvalidConflictState)
        ));
    }
    assert!(decode_conflict(Some("unknown".into()), id(), Some(vec![]), id()).is_err());
    assert!(decode_conflict(Some("merge".into()), Some(vec![0; 31]), Some(vec![]), id()).is_err());
    assert!(decode_conflict(None, id(), Some(vec![]), id()).is_err());
}
