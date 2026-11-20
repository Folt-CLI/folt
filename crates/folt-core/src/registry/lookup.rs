use rusqlite::{Connection, OptionalExtension};

use crate::config::names::RENAME_PENDING_PREFIX;
use crate::folio::format::FolioFormat;
use crate::folio::name::FolioName;
use crate::folio::row::FolioRow;
use crate::id::FolioId;
use crate::placement_path::RegistryError;

use super::Registry;
use super::codec::read_id;

impl Registry {
    pub fn format_version(&self) -> Result<Option<String>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        Ok(connection
            .query_row(
                "SELECT value FROM folt_metadata WHERE key='format_version'",
                [],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn folio_exists(&self, id: FolioId) -> Result<bool, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(false);
        };
        Ok(connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM folios WHERE id = ?1)",
            [id.as_bytes().as_slice()],
            |row| row.get(0),
        )?)
    }

    pub fn folio_row(&self, id: FolioId) -> Result<Option<FolioRow>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        let row = connection
            .query_row(
                "SELECT layout,algorithm FROM folios WHERE id=?1",
                [id.as_bytes().as_slice()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        row.map(|(text, algorithm)| {
            let layout = text
                .parse()
                .map_err(|_| RegistryError::InvalidLayout { id, text })?;
            Ok(FolioRow {
                id,
                format: FolioFormat { layout, algorithm },
            })
        })
        .transpose()
    }

    /// Returns the name, or an error if the Folio does not exist.
    pub fn folio_name(&self, id: FolioId) -> Result<Option<FolioName>, RegistryError> {
        let connection = self
            .connection
            .as_ref()
            .ok_or(RegistryError::FolioMissing(id))?;
        query_folio_name(connection, id)
    }

    /// Returns the interrupted rename owned by this Folio, if any.
    pub fn pending_rename(
        &self,
        id: FolioId,
    ) -> Result<Option<(FolioName, FolioName)>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        query_pending_rename(connection, id)
    }

    pub fn find_by_name(&self, name: &FolioName) -> Result<Option<FolioId>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        let bytes: Option<Vec<u8>> = connection
            .query_row(
                "SELECT id FROM folios WHERE name=?1",
                [name.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        bytes
            .map(|bytes| read_id(bytes).map(FolioId::from_bytes))
            .transpose()
    }

    pub fn folios(&self) -> Result<Vec<FolioId>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(Vec::new());
        };
        let mut statement = connection.prepare("SELECT id FROM folios ORDER BY id")?;
        let mut rows = statement.query([])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            result.push(FolioId::from_bytes(read_id(row.get(0)?)?));
        }
        Ok(result)
    }
}

pub(super) fn query_pending_rename(
    connection: &Connection,
    id: FolioId,
) -> Result<Option<(FolioName, FolioName)>, RegistryError> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM folt_metadata WHERE key=?1",
            [format!("{RENAME_PENDING_PREFIX}{id}")],
            |row| row.get(0),
        )
        .optional()?;
    value
        .map(|value| {
            let (old, new) = value
                .split_once('\n')
                .ok_or(RegistryError::InvalidPendingRename(id))?;
            let old = FolioName::parse(old).map_err(|_| RegistryError::InvalidPendingRename(id))?;
            let new = FolioName::parse(new).map_err(|_| RegistryError::InvalidPendingRename(id))?;
            Ok((old, new))
        })
        .transpose()
}

pub(super) fn query_folio_name(
    connection: &Connection,
    id: FolioId,
) -> Result<Option<FolioName>, RegistryError> {
    let name: Option<Option<String>> = connection
        .query_row(
            "SELECT name FROM folios WHERE id=?1",
            [id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    name.ok_or(RegistryError::FolioMissing(id))?
        .map(|text| FolioName::parse(&text).map_err(|_| RegistryError::InvalidName(text)))
        .transpose()
}
