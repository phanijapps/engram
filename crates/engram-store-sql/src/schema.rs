//! SQLite schema management for the SQL adapter.
//!
//! The first SQL schema stores accepted contract payloads losslessly as JSON
//! while also indexing scope and target fields needed by repository ports.

use engram_core::{CoreError, CoreResult};
use rusqlite::Connection;

/// Creates the SQLite tables required by the first SQL adapter slice.
///
/// The schema stores full contract JSON and only indexes fields needed for
/// lookup, scope filtering, event order, and idempotency.
pub(crate) fn initialize_schema(connection: &Connection) -> CoreResult<()> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS memory_events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                memory_id TEXT,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                event_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS write_idempotency (
                key TEXT PRIMARY KEY,
                response_json TEXT NOT NULL
            );
            "#,
        )
        .map_err(sql_error)
}

/// Converts SQLite errors into the stable core adapter error surface.
///
/// Adapter callers should not need to know which SQL engine produced a failure
/// in order to handle it consistently.
pub(crate) fn sql_error(error: rusqlite::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-sql".to_owned(),
        message: error.to_string(),
    }
}

/// Converts contract JSON serialization errors into a core adapter failure.
///
/// JSON failures indicate the adapter could not preserve an accepted payload
/// shape and should be surfaced as infrastructure errors.
pub(crate) fn json_error(error: serde_json::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-sql".to_owned(),
        message: error.to_string(),
    }
}
