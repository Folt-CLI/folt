use super::RegistryTransaction;
use crate::id::FolioId;
use crate::placement_path::RegistryError;
use crate::registry::{StoppedRebase, codec::encode_commits};

impl RegistryTransaction<'_> {
    /// Inserts progress inside the caller's transaction.
    pub fn insert_stopped_rebase(&mut self, row: &StoppedRebase) -> Result<(), RegistryError> {
        self.tx.execute("INSERT INTO stopped_rebases(id,branch,copy_path,original_tip,destination,remaining,current_replay,new_tip,file_before,marker_blob,discard) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)", rusqlite::params![row.id.as_bytes().as_slice(), row.branch, row.copy.as_bytes(), row.original_tip.as_bytes().as_slice(), row.destination.as_bytes().as_slice(), encode_commits(&row.remaining), row.current_replay.map(|id| id.as_bytes().to_vec()), row.new_tip.as_bytes().as_slice(), row.file_before.as_bytes().as_slice(), row.marker_blob.map(|id| id.as_bytes().to_vec()), row.discard])?;
        Ok(())
    }

    /// Replaces progress atomically inside the caller's transaction.
    pub fn update_stopped_rebase(&mut self, row: &StoppedRebase) -> Result<(), RegistryError> {
        self.remove_stopped_rebase(row.id)?;
        self.insert_stopped_rebase(row)
    }

    pub fn remove_stopped_rebase(&mut self, id: FolioId) -> Result<(), RegistryError> {
        self.tx.execute(
            "DELETE FROM stopped_rebases WHERE id=?1",
            [id.as_bytes().as_slice()],
        )?;
        Ok(())
    }
}
