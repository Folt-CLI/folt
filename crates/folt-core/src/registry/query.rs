use rusqlite::Connection;

use crate::id::FolioId;
use crate::placement_path::{PlacementPath, PlacementPrefix, RegistryError};

use super::codec::{prefix_boundary, prefix_successor};
use super::rows::{collect_placements, query_placement};
use super::{Placement, Registry};

pub(super) fn query_all_placements(
    connection: &Connection,
) -> Result<Vec<Placement>, RegistryError> {
    let mut statement = connection.prepare(
        "SELECT path,id,kind,head,commit_id,base,size,mtime,ctime,inode,staged,staged_size,conflict_operation,conflict_other,conflict_markers,conflict_pre_merge FROM placements ORDER BY path",
    )?;
    collect_placements(&mut statement.query([])?)
}

impl Registry {
    pub fn placement_at(&self, path: &PlacementPath) -> Result<Option<Placement>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        query_placement(
            connection,
            "SELECT path,id,kind,head,commit_id,base,size,mtime,ctime,inode,staged,staged_size,conflict_operation,conflict_other,conflict_markers,conflict_pre_merge FROM placements WHERE path=?1",
            &[&path.0],
        )
    }

    /// Returns the Folio's internal working-file row, if registered.
    pub fn internal_placement(&self, id: FolioId) -> Result<Option<Placement>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        query_placement(
            connection,
            "SELECT path,id,kind,head,commit_id,base,size,mtime,ctime,inode,staged,staged_size,conflict_operation,conflict_other,conflict_markers,conflict_pre_merge FROM placements WHERE id=?1 AND kind='internal'",
            &[&id.as_bytes().as_slice()],
        )
    }

    /// Includes internal rows for operations that must account for every placement.
    pub fn all_placements(&self) -> Result<Vec<Placement>, RegistryError> {
        match &self.connection {
            Some(connection) => query_all_placements(connection),
            None => Ok(Vec::new()),
        }
    }

    pub fn placements_under(
        &self,
        prefix: &PlacementPrefix,
    ) -> Result<Vec<Placement>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(Vec::new());
        };
        let lower = prefix_boundary(&prefix.0);
        let upper = prefix_successor(&lower);
        let mut statement = match upper {
            Some(_) => connection.prepare("SELECT path,id,kind,head,commit_id,base,size,mtime,ctime,inode,staged,staged_size,conflict_operation,conflict_other,conflict_markers,conflict_pre_merge FROM placements WHERE kind != 'internal' AND path >= ?1 AND path < ?2 ORDER BY path")?,
            None => connection.prepare("SELECT path,id,kind,head,commit_id,base,size,mtime,ctime,inode,staged,staged_size,conflict_operation,conflict_other,conflict_markers,conflict_pre_merge FROM placements WHERE kind != 'internal' AND path >= ?1 ORDER BY path")?,
        };
        let mut rows = if let Some(upper) = upper {
            statement.query(rusqlite::params![lower, upper])?
        } else {
            statement.query([&lower])?
        };
        collect_placements(&mut rows)
    }

    pub fn placements_of(&self, id: FolioId) -> Result<Vec<Placement>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(Vec::new());
        };
        let mut statement = connection
            .prepare("SELECT path,id,kind,head,commit_id,base,size,mtime,ctime,inode,staged,staged_size,conflict_operation,conflict_other,conflict_markers,conflict_pre_merge FROM placements WHERE id = ?1 AND kind != 'internal' ORDER BY path")?;
        let mut rows = statement.query([id.as_bytes().as_slice()])?;
        collect_placements(&mut rows)
    }

    /// Guards branch deletion, so includes the internal working-file row.
    pub fn placements_of_head(
        &self,
        id: FolioId,
        head: &str,
    ) -> Result<Vec<Placement>, RegistryError> {
        let Some(connection) = &self.connection else {
            return Ok(Vec::new());
        };
        let mut statement = connection.prepare(
            "SELECT path,id,kind,head,commit_id,base,size,mtime,ctime,inode,staged,staged_size,conflict_operation,conflict_other,conflict_markers,conflict_pre_merge FROM placements WHERE id=?1 AND head=?2 ORDER BY path",
        )?;
        let mut rows = statement.query(rusqlite::params![id.as_bytes().as_slice(), head])?;
        collect_placements(&mut rows)
    }
}
