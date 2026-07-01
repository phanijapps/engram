//! SQLite schema for the belief adapter.
//!
//! Each record is stored losslessly as contract JSON with scope columns indexed
//! for repository reads. Scope columns live on both beliefs and contradictions
//! (both carry their own scope).

use engram_runtime::{CoreError, CoreResult};
use rusqlite::Connection;

/// Creates the SQLite tables required by the belief adapter.
pub(crate) fn initialize_schema(connection: &Connection) -> CoreResult<()> {
    connection
        .execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;

            CREATE TABLE IF NOT EXISTS beliefs (
                id TEXT PRIMARY KEY,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS contradictions (
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
        adapter: "engram-store-belief-sqlite".to_owned(),
        message: error.to_string(),
    }
}

/// Converts contract JSON serialization errors into a core adapter failure.
pub(crate) fn json_error(error: serde_json::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-belief-sqlite".to_owned(),
        message: error.to_string(),
    }
}
