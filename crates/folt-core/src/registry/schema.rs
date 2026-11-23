use std::path::Path;

use rusqlite::Transaction;

use crate::config::formats::{APPLICATION_ID, SCHEMA_VERSION};
use crate::layout::HOME_FORMAT_VERSION;
use crate::placement_path::RegistryError;

pub(super) enum SchemaState {
    Empty,
    Current,
    Other,
}

pub(super) fn classify_schema(
    version: i64,
    app: i64,
    path: &Path,
) -> Result<SchemaState, RegistryError> {
    if version > SCHEMA_VERSION {
        return Err(RegistryError::UnsupportedSchemaVersion {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }
    if version == 0 && app == 0 {
        return Ok(SchemaState::Empty);
    }
    if app != i64::from(APPLICATION_ID) || version == 0 {
        return Err(RegistryError::ForeignDatabase(path.to_path_buf()));
    }
    if (1..SCHEMA_VERSION).contains(&version) {
        // This refusal must be removed before the first release, when migrations are required.
        return Err(RegistryError::OutdatedSchema {
            found: version,
            required: SCHEMA_VERSION,
            path: path.to_path_buf(),
        });
    }
    Ok(if version == SCHEMA_VERSION {
        SchemaState::Current
    } else {
        SchemaState::Other
    })
}

pub(super) fn create_schema(tx: &Transaction<'_>) -> Result<(), RegistryError> {
    tx.execute_batch("CREATE TABLE folios(id BLOB PRIMARY KEY, layout TEXT NOT NULL, algorithm TEXT NOT NULL, name TEXT) WITHOUT ROWID;\
        CREATE UNIQUE INDEX folios_name ON folios(name);\
        CREATE TABLE placements(path BLOB PRIMARY KEY, id BLOB NOT NULL REFERENCES folios(id), kind TEXT NOT NULL CHECK (kind IN ('link','copy','internal')), head TEXT NOT NULL DEFAULT 'main', commit_id BLOB, base BLOB, size INTEGER, mtime INTEGER, ctime INTEGER, inode INTEGER, staged BLOB, staged_size INTEGER, conflict_operation TEXT, conflict_other BLOB, conflict_markers BLOB, conflict_pre_merge BLOB, CHECK ((conflict_operation IS NULL AND conflict_other IS NULL AND conflict_markers IS NULL AND conflict_pre_merge IS NULL) OR (conflict_operation IS NOT NULL AND conflict_other IS NOT NULL AND conflict_markers IS NOT NULL AND conflict_pre_merge IS NOT NULL AND kind = 'copy')), CHECK ((staged IS NULL) = (staged_size IS NULL)), CHECK (staged_size IS NULL OR staged_size >= 0), CHECK (kind != 'internal' OR head = 'main'), CHECK (kind != 'link' OR (head = 'main' AND commit_id IS NULL AND base IS NULL AND staged IS NULL))) WITHOUT ROWID;\
        CREATE INDEX placements_id ON placements(id);\
        CREATE TABLE refs(parent BLOB NOT NULL, branch TEXT NOT NULL, key TEXT NOT NULL, child TEXT NOT NULL, child_label TEXT NOT NULL, pin TEXT, PRIMARY KEY(parent,branch,key)) WITHOUT ROWID;\
        CREATE INDEX refs_child ON refs(child);")?;
    tx.execute_batch("CREATE TABLE stopped_rebases(id BLOB PRIMARY KEY REFERENCES folios(id), branch TEXT NOT NULL, copy_path BLOB NOT NULL, original_tip BLOB NOT NULL, destination BLOB NOT NULL, remaining BLOB NOT NULL, current_replay BLOB, new_tip BLOB NOT NULL, file_before BLOB NOT NULL, marker_blob BLOB, discard INTEGER NOT NULL) WITHOUT ROWID;")?;
    tx.execute_batch(
        "CREATE TABLE folt_metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;",
    )?;
    tx.execute(
        "INSERT INTO folt_metadata(key,value) VALUES ('format_version',?1)",
        [HOME_FORMAT_VERSION.to_string()],
    )?;
    tx.pragma_update(None, "application_id", i64::from(APPLICATION_ID))?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}
