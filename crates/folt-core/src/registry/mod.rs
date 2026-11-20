use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::home::HomeLock;
#[cfg(test)]
use crate::placement_path::PlacementPath;
use crate::placement_path::RegistryError;

mod codec;
mod lookup;
mod open;
mod query;
mod references;
mod rows;
mod schema;
mod tx;

pub use crate::placement::PlacementBase;
pub use references::ReferenceRow;
pub(crate) use references::reference_name;
pub use rows::{NewPlacement, Placement, internal_placement};
pub use tx::{MoveResult, RegistryTransaction, RemoveResult};

/// SQLite registry for Folios and their placements.
pub struct Registry {
    connection: Option<Connection>,
    path: PathBuf,
    home: PathBuf,
}

impl Registry {
    pub(crate) fn home_path(&self) -> &Path {
        &self.home
    }
    #[cfg(test)]
    pub(crate) fn fail_writes_at(&self, path: &PlacementPath, insert: bool) {
        let operation = if insert { "INSERT" } else { "DELETE" };
        let row = if insert { "NEW" } else { "OLD" };
        let hex: String = path
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        self.connection.as_ref().unwrap().execute_batch(&format!(
            "CREATE TRIGGER injected_failure BEFORE {operation} ON placements WHEN {row}.path = X'{hex}' BEGIN SELECT RAISE(ABORT, 'injected failure'); END;"
        )).unwrap();
    }

    #[cfg(test)]
    pub(crate) fn clear_write_failure(&self) {
        self.connection
            .as_ref()
            .unwrap()
            .execute_batch("DROP TRIGGER injected_failure")
            .unwrap();
    }

    /// Runs a caller's read/decision/write operation in one immediate transaction.
    pub fn write<T>(
        &mut self,
        _lock: &HomeLock,
        operation: impl FnOnce(&mut RegistryTransaction<'_>) -> Result<T, RegistryError>,
    ) -> Result<T, RegistryError> {
        let connection = self
            .connection
            .as_mut()
            .ok_or_else(|| RegistryError::MissingDatabase(self.path.clone()))?;
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut registry_tx = RegistryTransaction { tx };
        let value = operation(&mut registry_tx)?;
        registry_tx.tx.commit()?;
        Ok(value)
    }

    /// Writes a consistent SQLite snapshot to a destination path.
    pub fn backup_to(
        &self,
        _lock: &HomeLock,
        destination: impl AsRef<Path>,
    ) -> Result<(), RegistryError> {
        let connection = self
            .connection
            .as_ref()
            .ok_or_else(|| RegistryError::MissingDatabase(self.path.clone()))?;
        let destination = destination.as_ref();
        let temporary = destination.with_extension(format!("{}.tmp", std::process::id()));
        let escaped = temporary.to_string_lossy().replace('\'', "''");
        connection.execute(&format!("VACUUM INTO '{escaped}'"), [])?;
        fs::rename(&temporary, destination)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
