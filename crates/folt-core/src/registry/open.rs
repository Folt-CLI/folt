use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::config::formats::{APPLICATION_ID, OUTDATED_SCHEMA, SCHEMA_VERSION};
use crate::config::names::REGISTRY_FILE;
use crate::home::{Home, HomeLock, is_regular_file};
use crate::placement_path::RegistryError;

use super::schema::check_schema;
use super::{Registry, schema};

impl Registry {
    /// Opens or creates the registry while the caller holds the home lock.
    pub fn open_or_create(home: &Home, _lock: &HomeLock) -> Result<Self, RegistryError> {
        let path = home.path().join(REGISTRY_FILE);
        match is_regular_file(&path) {
            Ok(true) => {}
            Ok(false) => return Err(RegistryError::NotRegularFile(path)),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(RegistryError::Io(source)),
        }
        let connection = Connection::open(&path)?;
        configure(&connection, true, home.busy_timeout())?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let app: i64 = connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(RegistryError::UnsupportedSchemaVersion {
                found: version,
                supported: SCHEMA_VERSION,
            });
        }
        if version != 0 && app != i64::from(APPLICATION_ID) {
            return Err(RegistryError::ForeignDatabase(path));
        }
        if version == OUTDATED_SCHEMA {
            // This refusal must be removed before the first release, when migrations are required.
            return Err(RegistryError::OutdatedSchema {
                found: version,
                required: SCHEMA_VERSION,
                path: path.clone(),
            });
        }
        if version == 0 {
            if app != 0 {
                return Err(RegistryError::ForeignDatabase(path));
            }
            let tx = connection.unchecked_transaction()?;
            schema::create_schema(&tx)?;
            tx.commit()?;
        }
        Ok(Self {
            connection: Some(connection),
            home: home.path().to_path_buf(),
            path,
        })
    }

    /// Opens an existing registry read-only. A missing registry is an empty view.
    pub fn open_existing(home: &Home) -> Result<Self, RegistryError> {
        let path = home.path().join(REGISTRY_FILE);
        let connection = match is_regular_file(&path) {
            Ok(true) => {
                let connection =
                    Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
                configure(&connection, false, home.busy_timeout())?;
                check_schema(&connection, &path)?;
                Some(connection)
            }
            Ok(false) => return Err(RegistryError::NotRegularFile(path)),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => return Err(RegistryError::Io(source)),
        };
        Ok(Self {
            connection,
            path,
            home: home.path().to_path_buf(),
        })
    }
}

fn configure(
    connection: &Connection,
    writer: bool,
    busy_timeout: Duration,
) -> Result<(), RegistryError> {
    connection.busy_timeout(busy_timeout)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    if writer {
        connection.pragma_update(None, "journal_mode", "WAL")?;
    }
    Ok(())
}
