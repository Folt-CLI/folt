use crate::folio::row::{FolioRow, NewFolio};
use crate::placement_path::{PlacementPrefix, RegistryError};

use super::super::Placement;
use super::super::codec::prefix_boundary;
use super::super::lookup::query_folio_name;
use super::{MoveResult, RegistryTransaction};

impl RegistryTransaction<'_> {
    pub fn move_prefix(
        &mut self,
        old: &PlacementPrefix,
        new: &PlacementPrefix,
    ) -> Result<MoveResult, RegistryError> {
        let lower = prefix_boundary(&old.0);
        if new.0 == old.0 || new.0.starts_with(&lower) {
            return Ok(MoveResult::InsideSource);
        }
        let rows = {
            let mut statement = self
                .tx
                .prepare("SELECT path FROM placements ORDER BY path")?;
            statement
                .query_map([], |row| row.get::<_, Vec<u8>>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let moves: Vec<_> = rows
            .iter()
            .filter(|path| path.starts_with(&lower))
            .map(|path| {
                let mut destination = new.0.clone();
                destination.extend_from_slice(&path[old.0.len()..]);
                (path, destination)
            })
            .collect();
        if moves.is_empty() {
            return Ok(MoveResult::NotRegistered);
        }
        if moves
            .iter()
            .any(|(_, destination)| rows.contains(destination))
        {
            return Ok(MoveResult::DestinationRegistered);
        }
        for (path, destination) in moves {
            self.tx.execute(
                "UPDATE placements SET path=?2 WHERE path=?1",
                rusqlite::params![path, destination],
            )?;
        }
        Ok(MoveResult::Moved)
    }
    pub fn replace_all(
        &mut self,
        folios: &[FolioRow],
        placements: &[Placement],
    ) -> Result<(), RegistryError> {
        // Names belong to Folio ids, even when placements and formats are rebuilt.
        let names = folios
            .iter()
            .map(|row| {
                if self.folio_exists(row.id)? {
                    Ok((row.id, query_folio_name(&self.tx, row.id)?))
                } else {
                    Ok((row.id, None))
                }
            })
            .collect::<Result<Vec<_>, RegistryError>>()?;
        self.tx.execute("DELETE FROM placements", [])?;
        self.tx.execute("DELETE FROM folios", [])?;
        for row in folios {
            self.add_folio(&NewFolio {
                id: row.id,
                format: row.format.clone(),
            })?;
        }
        for (id, name) in names {
            if let Some(name) = name {
                self.set_name(id, &name)?;
            }
        }
        for placement in placements {
            self.insert_placement(placement)?;
        }
        self.tx.execute(
            "DELETE FROM refs WHERE parent NOT IN (SELECT id FROM folios)",
            [],
        )?;
        Ok(())
    }
}
