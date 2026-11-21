use rusqlite::{Connection, OptionalExtension};

use crate::config::names::DEFAULT_BRANCH;
use crate::home::Home;
use crate::id::{FolioId, ObjectId};
use crate::placement::{PlacementBase, PlacementKind, StagedBlob};
use crate::placement_path::{PlacementPath, RegistryError};

use super::codec::{decode_base, decode_staged, read_id};

/// A placement stored by the registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Placement {
    pub path: PlacementPath,
    pub id: FolioId,
    pub kind: PlacementKind,
    pub head: String,
    pub commit_id: Option<ObjectId>,
    /// Links have no base; working files may have an uncaptured or cleared base.
    pub base: Option<PlacementBase>,
    pub staged: Option<StagedBlob>,
}

impl Placement {
    /// Converts a registry row to filesystem facts while retaining its Folio id.
    pub fn to_placement(&self) -> (FolioId, crate::placement::PlacementSpec) {
        (
            self.id,
            crate::placement::PlacementSpec {
                path: self.path.to_path_buf(),
                kind: self.kind,
                head: self.head.clone(),
                commit_id: self.commit_id,
                base: self.base.clone(),
            },
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewPlacement {
    pub path: PlacementPath,
    pub id: FolioId,
    pub kind: PlacementKind,
    /// Defaults to the layout's default branch when omitted.
    pub head: Option<String>,
    pub commit_id: Option<ObjectId>,
    pub base: Option<PlacementBase>,
    pub staged: Option<StagedBlob>,
}

/// Builds the initial internal row without registering it or capturing a base.
pub fn internal_placement(home: &Home, id: FolioId) -> Result<NewPlacement, RegistryError> {
    Ok(NewPlacement {
        path: PlacementPath::resolve(crate::folio::dir::path::content_path(home, id))?,
        id,
        kind: PlacementKind::Internal,
        head: Some(DEFAULT_BRANCH.into()),
        commit_id: None,
        base: None,
        staged: None,
    })
}

pub(super) fn query_placement(
    connection: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Option<Placement>, RegistryError> {
    connection
        .query_row(sql, params, read_placement)
        .optional()
        .map_err(Into::into)
}
fn read_placement(row: &rusqlite::Row<'_>) -> rusqlite::Result<Placement> {
    let invalid = |index, kind, error: RegistryError| {
        rusqlite::Error::FromSqlConversionFailure(index, kind, Box::new(error))
    };
    let kind: String = row.get(2)?;
    let commit: Option<Vec<u8>> = row.get(4)?;
    Ok(Placement {
        path: PlacementPath::from_stored(row.get(0)?),
        id: FolioId::from_bytes(
            read_id(row.get(1)?).map_err(|e| invalid(1, rusqlite::types::Type::Blob, e))?,
        ),
        kind: PlacementKind::parse(&kind).ok_or_else(|| {
            invalid(
                2,
                rusqlite::types::Type::Text,
                RegistryError::InvalidPlacementKind(kind),
            )
        })?,
        head: row.get(3)?,
        commit_id: commit
            .map(|bytes| {
                bytes.try_into().map(ObjectId::from_bytes).map_err(|_| {
                    invalid(
                        4,
                        rusqlite::types::Type::Blob,
                        RegistryError::InvalidCommitId,
                    )
                })
            })
            .transpose()?,
        base: decode_base(
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
            row.get(8)?,
            row.get(9)?,
        )
        .map_err(|e| invalid(5, rusqlite::types::Type::Blob, e))?,
        staged: decode_staged(row.get(10)?, row.get(11)?)
            .map_err(|e| invalid(10, rusqlite::types::Type::Blob, e))?,
    })
}
pub(super) fn collect_placements(
    rows: &mut rusqlite::Rows<'_>,
) -> Result<Vec<Placement>, RegistryError> {
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        result.push(read_placement(row)?);
    }
    Ok(result)
}
