use crate::folio::row::NewFolio;
use crate::id::FolioId;
use crate::placement_path::RegistryError;

use super::RegistryTransaction;

impl RegistryTransaction<'_> {
    pub fn folio_exists(&self, id: FolioId) -> Result<bool, RegistryError> {
        Ok(self.tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM folios WHERE id=?1)",
            [id.as_bytes().as_slice()],
            |row| row.get(0),
        )?)
    }

    pub fn add_folio(&mut self, folio: &NewFolio) -> Result<(), RegistryError> {
        self.tx.execute(
            "INSERT INTO folios(id,layout,algorithm) VALUES (?1,?2,?3)",
            rusqlite::params![
                folio.id.as_bytes().as_slice(),
                folio.format.layout.to_string(),
                folio.format.algorithm
            ],
        )?;
        Ok(())
    }

    /// Removes a Folio only when no live branch references it. The callback
    /// checks the parent's branch ref; placements also keep a branch live.
    /// Dead cache rows are purged only after all removal guards pass.
    pub fn remove_folio(
        &mut self,
        id: FolioId,
        is_live: &mut dyn FnMut(FolioId, &str) -> Result<bool, RegistryError>,
    ) -> Result<(), RegistryError> {
        let dead = self.check_referrers(id, "remove", is_live)?;
        let count: i64 = self.tx.query_row(
            "SELECT count(*) FROM placements WHERE id=?1 AND kind != 'internal'",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        if count != 0 {
            return Err(RegistryError::FolioHasPlacements(id));
        }
        for (parent, branch) in dead {
            self.delete_branch_references(parent, &branch)?;
        }
        self.tx.execute(
            "DELETE FROM placements WHERE id=?1 AND kind='internal'",
            [id.as_bytes().as_slice()],
        )?;
        self.tx.execute(
            "DELETE FROM refs WHERE parent=?1",
            [id.as_bytes().as_slice()],
        )?;
        self.clear_pending_rename(id)?;
        self.tx
            .execute("DELETE FROM folios WHERE id=?1", [id.as_bytes().as_slice()])?;
        Ok(())
    }
}
