//! SQLite schema for the operator proposal queue cell.
//!
//! Proposals persist as contract JSON with scope/status/version columns
//! indexed for queue reads and compare-and-set transitions. The cell owns
//! ONLY these tables — never memory tables, even when a deployment shares
//! one database file.

use engram_runtime::{CoreError, CoreResult};
use rusqlite::Connection;

/// Creates the SQLite tables required by the proposals cell.
pub(crate) fn initialize_schema(connection: &Connection) -> CoreResult<()> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS operator_proposals (
                id TEXT PRIMARY KEY,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                status TEXT NOT NULL,
                version INTEGER NOT NULL,
                payload_fingerprint TEXT NOT NULL,
                submission_key TEXT,
                created_at TEXT NOT NULL,
                record_json TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_operator_proposals_submission
                ON operator_proposals (tenant, subject, workspace, session, environment, submission_key)
                WHERE submission_key IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_operator_proposals_queue
                ON operator_proposals (tenant, subject, workspace, session, environment, status, created_at, id);
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
