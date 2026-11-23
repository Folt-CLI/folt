use super::{Registry, codec::decode_commits};
use crate::id::{FolioId, ObjectId};
use crate::placement_path::{PlacementPath, RegistryError};
use rusqlite::OptionalExtension;

/// Durable progress for the one stopped rebase of a Folio.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoppedRebase {
    pub id: FolioId,
    pub branch: String,
    pub copy: PlacementPath,
    pub original_tip: ObjectId,
    pub destination: ObjectId,
    pub remaining: Vec<ObjectId>,
    /// None means replay finished and placement movement may need recovery.
    pub current_replay: Option<ObjectId>,
    pub new_tip: ObjectId,
    /// File at the publication boundary; equals marker_blob after acknowledgment.
    pub file_before: ObjectId,
    pub marker_blob: Option<ObjectId>,
    pub discard: bool,
}

impl Registry {
    /// Reads the stopped rebase of one Folio, including pending completion.
    pub fn stopped_rebase(&self, id: FolioId) -> Result<Option<StoppedRebase>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        let mut statement = connection.prepare("SELECT branch,copy_path,original_tip,destination,remaining,current_replay,new_tip,file_before,marker_blob,discard FROM stopped_rebases WHERE id=?1")?;
        let value = statement
            .query_row([id.as_bytes().as_slice()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                    row.get::<_, Vec<u8>>(7)?,
                    row.get::<_, Option<Vec<u8>>>(8)?,
                    row.get::<_, bool>(9)?,
                ))
            })
            .optional()?;
        let object = |bytes: Vec<u8>| -> Result<ObjectId, RegistryError> {
            Ok(ObjectId::from_bytes(
                bytes
                    .try_into()
                    .map_err(|_| RegistryError::InvalidConflictState)?,
            ))
        };
        value
            .map(
                |(
                    branch,
                    copy,
                    original,
                    destination,
                    remaining,
                    current,
                    tip,
                    before,
                    marker,
                    discard,
                )| {
                    Ok(StoppedRebase {
                        id,
                        branch,
                        copy: PlacementPath::from_stored(copy),
                        original_tip: object(original)?,
                        destination: object(destination)?,
                        remaining: decode_commits(&remaining)?,
                        current_replay: current.map(object).transpose()?,
                        new_tip: object(tip)?,
                        file_before: object(before)?,
                        marker_blob: marker.map(object).transpose()?,
                        discard,
                    })
                },
            )
            .transpose()
    }
}
