use std::path::Path;

use rusqlite::{Connection, Transaction};

use crate::config::formats::{APPLICATION_ID, OUTDATED_SCHEMA, SCHEMA_VERSION};
use crate::layout::HOME_FORMAT_VERSION;
use crate::placement_path::RegistryError;

pub(super) fn check_schema(connection: &Connection, path: &Path) -> Result<(), RegistryError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let app: i64 = connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(RegistryError::UnsupportedSchemaVersion {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }
    if app != i64::from(APPLICATION_ID) {
        return Err(RegistryError::ForeignDatabase(path.to_path_buf()));
    }
    if version == OUTDATED_SCHEMA {
        return Err(RegistryError::OutdatedSchema {
            found: version,
            required: SCHEMA_VERSION,
            path: path.to_path_buf(),
        });
    }
    if version != SCHEMA_VERSION {
        return Err(RegistryError::ForeignDatabase(path.to_path_buf()));
    }
    Ok(())
}

pub(super) fn create_schema(tx: &Transaction<'_>) -> Result<(), RegistryError> {
    tx.execute_batch("CREATE TABLE folios(id BLOB PRIMARY KEY, layout TEXT NOT NULL, algorithm TEXT NOT NULL, name TEXT) WITHOUT ROWID;\
        CREATE UNIQUE INDEX folios_name ON folios(name);\
        CREATE TABLE placements(path BLOB PRIMARY KEY, id BLOB NOT NULL REFERENCES folios(id), kind TEXT NOT NULL CHECK (kind IN ('link','copy','internal')), head TEXT NOT NULL DEFAULT 'main', commit_id BLOB, base BLOB, size INTEGER, mtime INTEGER, ctime INTEGER, inode INTEGER, staged BLOB, staged_size INTEGER, CHECK ((staged IS NULL) = (staged_size IS NULL)), CHECK (staged_size IS NULL OR staged_size >= 0), CHECK (kind != 'internal' OR head = 'main'), CHECK (kind != 'link' OR (head = 'main' AND commit_id IS NULL AND base IS NULL AND staged IS NULL))) WITHOUT ROWID;\
        CREATE INDEX placements_id ON placements(id);\
        CREATE TABLE refs(parent BLOB NOT NULL, branch TEXT NOT NULL, key TEXT NOT NULL, child TEXT NOT NULL, child_label TEXT NOT NULL, pin TEXT, PRIMARY KEY(parent,branch,key)) WITHOUT ROWID;\
        CREATE INDEX refs_child ON refs(child);")?;
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
