//! SQLite schema for the procedures adapter.
//!
//! Each record is stored losslessly as contract JSON with scope columns indexed
//! for repository reads (mirrors the belief/knowledge adapters).

use engram_runtime::{CoreError, CoreResult};
use rusqlite::Connection;

/// Creates the SQLite table required by the procedures adapter.
pub(crate) fn initialize_schema(connection: &Connection) -> CoreResult<()> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS procedures (
                id TEXT PRIMARY KEY,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                record_json TEXT NOT NULL
            );
            "#,
        )
        .map_err(sql_error)
}

/// Converts SQLite errors into the stable core adapter error surface.
pub(crate) fn sql_error(error: rusqlite::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-sqlite".to_owned(),
        message: error.to_string(),
    }
}

/// Converts contract JSON serialization errors into a core adapter failure.
pub(crate) fn json_error(error: serde_json::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-sqlite".to_owned(),
        message: error.to_string(),
    }
}
