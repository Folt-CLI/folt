use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::fsx::TempFile;
use crate::home::HomeLock;
use crate::placement_path::RegistryError;

mod codec;
mod lookup;
mod open;
mod query;
mod references;
mod rows;
mod schema;
mod stopped;
mod tx;

pub use crate::placement::PlacementBase;
pub use references::ReferenceRow;
pub(crate) use references::reference_name;
pub use rows::{NewPlacement, Placement, internal_placement};
pub use stopped::StoppedRebase;
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
        // VACUUM INTO refuses an existing file, so the guard holds the name without creating it.
        let temporary = TempFile::sibling_at(
            &destination.with_extension(format!("{}.tmp", std::process::id())),
        )?;
        let escaped = temporary.path().to_string_lossy().replace('\'', "''");
        connection.execute(&format!("VACUUM INTO '{escaped}'"), [])?;
        temporary.publish(destination)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
