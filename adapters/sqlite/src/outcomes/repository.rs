//! SQLite outcome ledger (F2): append-only writes + windowed reads.
//!
//! Telemetry, not memory: rows are scoped, policy-light diagnostics. The
//! ledger never feeds retrieval lanes and never claims causal credit.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use engram_domain::{
    OutcomeWindow, RecallOutcome, RecordedOutcome, ReportOutcomeRequest, ReportOutcomeResult,
};
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};

use crate::outcomes::schema::{initialize_schema, json_error, sql_error};
use crate::{SqliteOpenOptions, SqlitePath};

#[derive(Clone)]
pub struct SqlOutcomeStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqlOutcomeStore {
    pub fn open_in_memory() -> CoreResult<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(sql_error)?)
    }

    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        Self::open_with_options(SqliteOpenOptions::file_wal_concurrent(
            path.as_ref().to_path_buf(),
        ))
    }

    pub fn open_with_options(options: SqliteOpenOptions) -> CoreResult<Self> {
        let connection = match &options.path {
            SqlitePath::File(path) => Connection::open(path),
            SqlitePath::InMemory => Connection::open_in_memory(),
        }
        .map_err(sql_error)?;
        connection
            .query_row("PRAGMA journal_mode = WAL", [], |_r| Ok(()))
            .optional()
            .map_err(sql_error)?;
        connection
            .query_row("PRAGMA busy_timeout = 5000", [], |_r| Ok(()))
            .optional()
            .map_err(sql_error)?;
        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn from_connection(connection: Connection) -> CoreResult<Self> {
        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn lock(&self) -> CoreResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| CoreError::Adapter {
            adapter: "engram-store-sqlite".to_owned(),
            message: "outcome ledger connection poisoned".to_owned(),
        })
    }
}

#[async_trait]
impl engram_retrieval::OutcomeLedger for SqlOutcomeStore {
    async fn report_outcome(
        &self,
        request: ReportOutcomeRequest,
    ) -> CoreResult<ReportOutcomeResult> {
        let conn = self.lock()?;
        let json = serde_json::to_string(&request.outcome).map_err(json_error)?;
        let occurred = request.outcome.occurred_at.to_rfc3339();
        conn.execute(
            "INSERT INTO recall_outcomes (query_id, tenant, subject, workspace, session, environment, success, capability, occurred_at, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params!(
                request.outcome.query_id,
                request.outcome.scope.tenant,
                request.outcome.scope.subject,
                request.outcome.scope.workspace,
                request.outcome.scope.session,
                request.outcome.scope.environment,
                request.outcome.outcome.success as i64,
                request.outcome.outcome.capability,
                occurred,
                json,
            ),
        )
        .map_err(sql_error)?;
        let row_id: i64 = conn.last_insert_rowid();
        Ok(ReportOutcomeResult {
            recorded: RecordedOutcome {
                id: engram_domain::Id::from(format!("outcome-{row_id}")),
                outcome: request.outcome,
            },
        })
    }

    async fn list_outcomes(&self, window: &OutcomeWindow) -> CoreResult<Vec<RecordedOutcome>> {
        let conn = self.lock()?;
        let since = window.since.to_rfc3339();
        let until = window.until.to_rfc3339();
        // ALL scope dimensions filter IN SQL before ORDER/LIMIT (review P0 +
        // P1-3: a post-limit Rust filter erased quiet workspaces once an
        // active sibling wrote 500 newer rows; session/environment were
        // never checked). Scope semantics mirror scope_allows: an ABSENT
        // request field matches every stored value; a PRESENT one requires
        // equality.
        let mut sql = "SELECT rowid, record_json FROM recall_outcomes WHERE tenant = ?1 AND occurred_at >= ?2 AND occurred_at <= ?3".to_owned();
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(window.scope.tenant.clone()),
            Box::new(since),
            Box::new(until),
        ];
        let mut idx = 4;
        for (col, value) in [
            ("subject", &window.scope.subject),
            ("workspace", &window.scope.workspace),
            ("session", &window.scope.session),
            ("environment", &window.scope.environment),
        ] {
            if let Some(v) = value {
                sql.push_str(&format!(" AND {col} = ?{idx}"));
                binds.push(Box::new(v.clone()));
                idx += 1;
            }
        }
        sql.push_str(" ORDER BY occurred_at DESC LIMIT 500");
        let mut stmt = conn.prepare(&sql).map_err(sql_error)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(binds.iter()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        let mut out = Vec::new();
        for row in rows {
            let (row_id, json) = row.map_err(sql_error)?;
            let outcome: RecallOutcome = serde_json::from_str(&json).map_err(json_error)?;
            out.push(RecordedOutcome {
                id: engram_domain::Id::from(format!("outcome-{row_id}")),
                outcome,
            });
        }
        Ok(out)
    }
}

/// Reads the newest outcome's timestamp (retention/diagnostic helper).
#[allow(dead_code)]
pub(crate) fn latest_outcome_at(
    conn: &Connection,
    tenant: &str,
) -> CoreResult<Option<DateTime<Utc>>> {
    conn.query_row(
        "SELECT occurred_at FROM recall_outcomes WHERE tenant = ?1 ORDER BY occurred_at DESC LIMIT 1",
        [tenant],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(sql_error)?
    .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
    .transpose()
    .map_err(|e| CoreError::Adapter {
        adapter: "engram-store-sqlite".to_owned(),
        message: e.to_string(),
    })
}
