use rusqlite::OptionalExtension;

use crate::config::names::RENAME_PENDING_PREFIX;
use crate::folio_name::FolioName;
use crate::id::FolioId;
use crate::placement_path::RegistryError;

use super::super::codec::read_id;
use super::super::lookup::{query_folio_name, query_pending_rename};
use super::RegistryTransaction;

/// SQLite unique constraint extended result code (dimensionless).
const SQLITE_CONSTRAINT_UNIQUE: i32 = 2067;

impl RegistryTransaction<'_> {
    pub fn pending_rename(
        &self,
        id: FolioId,
    ) -> Result<Option<(FolioName, FolioName)>, RegistryError> {
        query_pending_rename(&self.tx, id)
    }

    pub fn set_pending_rename(
        &mut self,
        id: FolioId,
        old: &FolioName,
        new: &FolioName,
    ) -> Result<(), RegistryError> {
        query_folio_name(&self.tx, id)?;
        self.tx.execute(
            "INSERT INTO folt_metadata(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            rusqlite::params![format!("{RENAME_PENDING_PREFIX}{id}"), format!("{old}\n{new}")],
        )?;
        Ok(())
    }

    pub fn clear_pending_rename(&mut self, id: FolioId) -> Result<(), RegistryError> {
        self.tx.execute(
            "DELETE FROM folt_metadata WHERE key=?1",
            [format!("{RENAME_PENDING_PREFIX}{id}")],
        )?;
        Ok(())
    }

    pub fn find_by_name(&self, name: &FolioName) -> Result<Option<FolioId>, RegistryError> {
        let bytes: Option<Vec<u8>> = self
            .tx
            .query_row(
                "SELECT id FROM folios WHERE name=?1",
                [name.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        bytes
            .map(|b| read_id(b).map(FolioId::from_bytes))
            .transpose()
    }

    /// Replaces a name and returns the previous name. The unique index uses binary collation.
    pub fn set_name(
        &mut self,
        id: FolioId,
        name: &FolioName,
    ) -> Result<Option<FolioName>, RegistryError> {
        let previous = query_folio_name(&self.tx, id)?;
        if previous.as_ref() == Some(name) {
            return Ok(previous);
        }
        self.tx
            .execute(
                "UPDATE folios SET name=?2 WHERE id=?1",
                rusqlite::params![id.as_bytes().as_slice(), name.as_str()],
            )
            .map_err(|error| match error {
                rusqlite::Error::SqliteFailure(ref code, _)
                    if code.extended_code == SQLITE_CONSTRAINT_UNIQUE =>
                {
                    RegistryError::NameTaken(name.clone())
                }
                other => RegistryError::Sqlite(other),
            })?;
        Ok(previous)
    }

    /// Renames an already named Folio and returns its old name.
    pub fn rename(&mut self, id: FolioId, name: &FolioName) -> Result<FolioName, RegistryError> {
        let previous = query_folio_name(&self.tx, id)?.ok_or(RegistryError::NotNamed(id))?;
        self.set_name(id, name)?;
        Ok(previous)
    }

    pub fn clear_name(&mut self, id: FolioId) -> Result<Option<FolioName>, RegistryError> {
        let previous = query_folio_name(&self.tx, id)?;
        self.tx.execute(
            "UPDATE folios SET name=NULL WHERE id=?1",
            [id.as_bytes().as_slice()],
        )?;
        Ok(previous)
    }
}
