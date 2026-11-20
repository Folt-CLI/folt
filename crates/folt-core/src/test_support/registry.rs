use rusqlite::Connection;

use crate::config::names::REGISTRY_FILE;
use crate::placement_path::PlacementPath;
use crate::registry::Registry;

pub(crate) fn fail_writes_at(registry: &Registry, path: &PlacementPath, insert: bool) {
    let operation = if insert { "INSERT" } else { "DELETE" };
    let row = if insert { "NEW" } else { "OLD" };
    let hex: String = path
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Connection::open(registry.home_path().join(REGISTRY_FILE)).unwrap().execute_batch(&format!(
            "CREATE TRIGGER injected_failure BEFORE {operation} ON placements WHEN {row}.path = X'{hex}' BEGIN SELECT RAISE(ABORT, 'injected failure'); END;"
        )).unwrap();
}

pub(crate) fn clear_write_failure(registry: &Registry) {
    Connection::open(registry.home_path().join(REGISTRY_FILE))
        .unwrap()
        .execute_batch("DROP TRIGGER injected_failure")
        .unwrap();
}
