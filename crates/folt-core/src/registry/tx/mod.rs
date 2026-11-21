use rusqlite::Transaction;

mod folios;
mod moves;
mod names;
mod placements;
mod references;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MoveResult {
    Moved,
    NotRegistered,
    DestinationRegistered,
    InsideSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoveResult {
    Removed,
    NotRegistered,
}

/// Operations available inside a single registry transaction.
pub struct RegistryTransaction<'a> {
    pub(super) tx: Transaction<'a>,
}
