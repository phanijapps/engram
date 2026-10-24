//! Schema for the outcome-ledger cell (F2): one append-only table.

use engram_runtime::{CoreError, CoreResult};
use rusqlite::Connection;

pub(crate) fn initialize_schema(connection: &Connection) -> CoreResult<()> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS recall_outcomes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                query_id TEXT NOT NULL,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                success INTEGER NOT NULL,
                capability TEXT,
                occurred_at TEXT NOT NULL,
                record_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_recall_outcomes_scope_time
                ON recall_outcomes (tenant, subject, workspace, session, environment, occurred_at);
            CREATE INDEX IF NOT EXISTS idx_recall_outcomes_query
                ON recall_outcomes (query_id);
            "#,
        )
        .map_err(sql_error)
}

pub(crate) fn sql_error(error: rusqlite::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-sqlite".to_owned(),
        message: error.to_string(),
    }
}

pub(crate) fn json_error(error: serde_json::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-sqlite".to_owned(),
        message: error.to_string(),
    }
}
