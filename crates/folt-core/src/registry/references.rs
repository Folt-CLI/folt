use crate::folio_name::FolioName;
use crate::id::FolioId;
use crate::placement_path::RegistryError;

use super::Registry;

/// Use the same name grammar for planning and reference-cache rebuilds.
pub(crate) fn reference_name(
    reference: &crate::front_matter::Reference<'_>,
) -> Result<Option<FolioName>, RegistryError> {
    reference
        .name
        .as_deref()
        .map(FolioName::parse)
        .transpose()
        .map_err(|e| RegistryError::References(format!("reference '{}': {e}", reference.alias)))
}

/// A declared edge cached by the last stage of a Folio branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceRow {
    pub key: String,
    /// Canonical declared/resolved id, or empty for an unresolved name.
    pub child: String,
    pub child_label: String,
    pub pin: Option<String>,
}

impl Registry {
    pub fn references(
        &self,
        parent: FolioId,
        branch: &str,
    ) -> Result<Vec<ReferenceRow>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(Vec::new());
        };
        let mut statement = connection.prepare(
            "SELECT key,child,child_label,pin FROM refs WHERE parent=?1 AND branch=?2 ORDER BY key",
        )?;
        let rows = statement.query_map(
            rusqlite::params![parent.as_bytes().as_slice(), branch],
            |r| {
                Ok(ReferenceRow {
                    key: r.get(0)?,
                    child: r.get(1)?,
                    child_label: r.get(2)?,
                    pin: r.get(3)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}
