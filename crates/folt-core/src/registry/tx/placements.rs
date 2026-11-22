use crate::config::names::DEFAULT_BRANCH;
use crate::placement::PlacementBase;
use crate::placement_path::{PlacementPath, RegistryError};

use super::super::codec::{encode_base, encode_conflict, encode_staged};
use super::super::query::query_all_placements;
use super::super::rows::query_placement;
use super::super::{NewPlacement, Placement};
use super::{MoveResult, RegistryTransaction, RemoveResult};

impl RegistryTransaction<'_> {
    pub fn all_placements(&self) -> Result<Vec<Placement>, RegistryError> {
        query_all_placements(&self.tx)
    }

    pub fn placement_at(&self, path: &PlacementPath) -> Result<Option<Placement>, RegistryError> {
        query_placement(
            &self.tx,
            "SELECT path,id,kind,head,commit_id,base,size,mtime,ctime,inode,staged,staged_size,conflict_operation,conflict_other,conflict_markers,conflict_pre_merge FROM placements WHERE path=?1",
            &[&path.0],
        )
    }
    pub fn add_placement(&mut self, placement: &NewPlacement) -> Result<(), RegistryError> {
        self.insert_placement(&Placement {
            path: placement.path.clone(),
            id: placement.id,
            kind: placement.kind,
            head: placement.head.as_deref().unwrap_or(DEFAULT_BRANCH).into(),
            commit_id: placement.commit_id,
            base: placement.base.clone(),
            staged: placement.staged,
            conflict: None,
        })
    }
    pub(crate) fn insert_placement(&mut self, placement: &Placement) -> Result<(), RegistryError> {
        let encoded = encode_base(placement.base.as_ref())?;
        let (staged, staged_size) = encode_staged(placement.staged)?;
        let conflict = encode_conflict(placement.conflict.as_ref())?;
        self.tx.execute(
            "INSERT INTO placements(path,id,kind,head,commit_id,base,size,mtime,ctime,inode,staged,staged_size,conflict_operation,conflict_other,conflict_markers,conflict_pre_merge) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            rusqlite::params![placement.path.0, placement.id.as_bytes().as_slice(), placement.kind.as_str(), placement.head, placement.commit_id.map(|id| id.as_bytes().to_vec()), encoded.hash, encoded.size, encoded.mtime, encoded.ctime, encoded.inode, staged, staged_size, conflict.operation, conflict.other, conflict.markers, conflict.pre_merge],
        )?;
        Ok(())
    }
    pub fn update_placement(&mut self, placement: &Placement) -> Result<(), RegistryError> {
        let encoded = encode_base(placement.base.as_ref())?;
        let (staged, staged_size) = encode_staged(placement.staged)?;
        let conflict = encode_conflict(placement.conflict.as_ref())?;
        self.tx.execute(
            "UPDATE placements SET id=?2,kind=?3,head=?4,commit_id=?5,base=?6,size=?7,mtime=?8,ctime=?9,inode=?10,staged=?11,staged_size=?12,conflict_operation=?13,conflict_other=?14,conflict_markers=?15,conflict_pre_merge=?16 WHERE path=?1",
            rusqlite::params![placement.path.0, placement.id.as_bytes().as_slice(), placement.kind.as_str(), placement.head, placement.commit_id.map(|id| id.as_bytes().to_vec()), encoded.hash, encoded.size, encoded.mtime, encoded.ctime, encoded.inode, staged, staged_size, conflict.operation, conflict.other, conflict.markers, conflict.pre_merge],
        )?;
        Ok(())
    }
    pub(crate) fn update_conflict_state(
        &mut self,
        path: &PlacementPath,
        state: &crate::placement::ConflictState,
    ) -> Result<(), RegistryError> {
        let encoded = encode_conflict(Some(state))?;
        self.tx.execute(
            "UPDATE placements SET conflict_operation=?2,conflict_other=?3,conflict_markers=?4,conflict_pre_merge=?5 WHERE path=?1",
            rusqlite::params![path.0, encoded.operation, encoded.other, encoded.markers, encoded.pre_merge],
        )?;
        Ok(())
    }

    pub fn update_placement_base(
        &mut self,
        path: &PlacementPath,
        base: Option<&PlacementBase>,
    ) -> Result<(), RegistryError> {
        let encoded = encode_base(base)?;
        self.tx.execute(
            "UPDATE placements SET base=?2,size=?3,mtime=?4,ctime=?5,inode=?6 WHERE path=?1",
            rusqlite::params![
                path.0,
                encoded.hash,
                encoded.size,
                encoded.mtime,
                encoded.ctime,
                encoded.inode
            ],
        )?;
        Ok(())
    }
    pub fn remove_placement(
        &mut self,
        path: &PlacementPath,
    ) -> Result<RemoveResult, RegistryError> {
        let count = self
            .tx
            .execute("DELETE FROM placements WHERE path=?1", [&path.0])?;
        Ok(if count == 0 {
            RemoveResult::NotRegistered
        } else {
            RemoveResult::Removed
        })
    }
    pub fn move_placement(
        &mut self,
        old: &PlacementPath,
        new: &PlacementPath,
    ) -> Result<MoveResult, RegistryError> {
        let exists = |path: &PlacementPath| -> Result<bool, RegistryError> {
            Ok(self.tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM placements WHERE path=?1)",
                [&path.0],
                |row| row.get(0),
            )?)
        };
        if !exists(old)? {
            return Ok(MoveResult::NotRegistered);
        }
        if exists(new)? {
            return Ok(MoveResult::DestinationRegistered);
        }
        self.tx.execute(
            "UPDATE placements SET path=?2 WHERE path=?1",
            rusqlite::params![old.0, new.0],
        )?;
        Ok(MoveResult::Moved)
    }
}
