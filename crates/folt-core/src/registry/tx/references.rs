use crate::folio::name::FolioName;
use crate::id::FolioId;
use crate::placement_path::RegistryError;

use super::super::codec::read_id;
use super::super::lookup::query_folio_name;
use super::super::reference_name;
use super::RegistryTransaction;

impl RegistryTransaction<'_> {
    pub fn delete_branch_references(
        &mut self,
        parent: FolioId,
        branch: &str,
    ) -> Result<(), RegistryError> {
        self.tx.execute(
            "DELETE FROM refs WHERE parent=?1 AND branch=?2",
            rusqlite::params![parent.as_bytes().as_slice(), branch],
        )?;
        Ok(())
    }

    fn branch_has_placements(&self, parent: FolioId, branch: &str) -> Result<bool, RegistryError> {
        Ok(self.tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM placements WHERE id=?1 AND head=?2)",
            rusqlite::params![parent.as_bytes().as_slice(), branch],
            |row| row.get(0),
        )?)
    }

    pub fn purge_dead_branch_references(
        &mut self,
        parent: FolioId,
        live_branches: &[&str],
    ) -> Result<(), RegistryError> {
        let branches = self
            .tx
            .prepare("SELECT DISTINCT branch FROM refs WHERE parent=?1")?
            .query_map([parent.as_bytes().as_slice()], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for branch in branches {
            if !live_branches.contains(&branch.as_str())
                && !self.branch_has_placements(parent, &branch)?
            {
                self.delete_branch_references(parent, &branch)?;
            }
        }
        Ok(())
    }

    pub fn rebuild_references(
        &mut self,
        parent: FolioId,
        branch: &str,
        references: &[crate::front_matter::Reference<'_>],
    ) -> Result<(), RegistryError> {
        self.delete_branch_references(parent, branch)?;
        for reference in references {
            let name = reference_name(reference)?;
            let child = match reference.id {
                Some(id) => Some(id),
                None => match &name {
                    Some(name) => self.find_by_name(name)?,
                    None => None,
                },
            };
            self.tx.execute(
                "INSERT INTO refs(parent,branch,key,child,child_label,pin) VALUES (?1,?2,?3,?4,?5,?6)",
                rusqlite::params![parent.as_bytes().as_slice(), branch, reference.alias,
                    child.map(|id| id.to_string()).unwrap_or_default(),
                    reference.name.as_deref().unwrap_or(""), reference.pin.as_deref()],
            )?;
        }
        Ok(())
    }

    /// Shared read-only guard for removal and rename. Returns dead branches so
    /// removal can purge them only after its remaining guards pass.
    pub(crate) fn check_referrers(
        &self,
        id: FolioId,
        operation: &str,
        is_live: &mut dyn FnMut(FolioId, &str) -> Result<bool, RegistryError>,
    ) -> Result<Vec<(FolioId, String)>, RegistryError> {
        let name = query_folio_name(&self.tx, id)?;
        let mut statement = self.tx.prepare("SELECT DISTINCT parent,branch,key FROM refs WHERE child=?1 OR (child='' AND child_label=?2) ORDER BY parent,branch,key")?;
        let referrers = statement
            .query_map(
                rusqlite::params![id.to_string(), name.as_ref().map(FolioName::as_str)],
                |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )?
            .map(|r| {
                let (parent, branch, key) = r?;
                Ok((FolioId::from_bytes(read_id(parent)?), branch, key))
            })
            .collect::<Result<Vec<_>, RegistryError>>()?;
        drop(statement);
        let mut dead = Vec::new();
        let mut live = Vec::new();
        for (parent, branch, key) in referrers {
            if self.branch_has_placements(parent, &branch)? || is_live(parent, &branch)? {
                live.push(format!("{parent} ({branch}, {key})"));
            } else {
                dead.push((parent, branch));
            }
        }
        if !live.is_empty() {
            return Err(RegistryError::References(format!(
                "cannot {operation} Folio {id}; referenced by {}",
                live.join(", ")
            )));
        }
        Ok(dead)
    }
}
