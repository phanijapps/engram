//! SQLite schema management for the SQL adapter.
//!
//! The first SQL schema stores accepted contract payloads losslessly as JSON
//! while also indexing scope and target fields needed by repository ports.

use engram_core::{CoreError, CoreResult};
use rusqlite::Connection;

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

pub(crate) fn sql_error(error: rusqlite::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-sql".to_owned(),
        message: error.to_string(),
    }
}

pub(crate) fn json_error(error: serde_json::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-sql".to_owned(),
        message: error.to_string(),
    }
}
