//! SQLite-backed procedure repository (RFC-0016 Layer 6).
//!
//! Storage-only: persists procedure records as contract JSON with scope
//! indexing. Success/failure accounting is caller-driven — the server records
//! outcomes, it does not decide when a procedure applies.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::*;
use engram_procedures::ProcedureRepository;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};

use crate::procedures::{
    schema::{initialize_schema, json_error, sql_error},
    scope::scope_allows,
};
use crate::{SqliteOpenOptions, SqlitePath};

/// SQLite-backed procedure repository.
///
/// Preserves procedure payloads as contract JSON while indexing identifiers and
/// scope columns for repository reads.
#[derive(Clone)]
pub struct SqlProcedureStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqlProcedureStore {
    /// Opens an in-memory procedure store and initializes its schema.
    pub fn open_in_memory() -> CoreResult<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(sql_error)?)
    }

    /// Opens a file-backed procedure store and initializes its schema.
    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        Self::open_with_options(SqliteOpenOptions::file_wal_concurrent(
            path.as_ref().to_path_buf(),
        ))
    }

    /// Opens a SQLite procedure store with explicit configuration options.
    pub fn open_with_options(options: SqliteOpenOptions) -> CoreResult<Self> {
        let connection = match &options.path {
            SqlitePath::File(path) => {
                if options.create_parent_dirs {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| CoreError::Adapter {
                            adapter: "engram-store-sqlite".to_owned(),
                            message: format!("failed to create parent directory: {e}"),
                        })?;
                    }
                }
                Connection::open(path)
            }
            SqlitePath::InMemory => Connection::open_in_memory(),
        }
        .map_err(sql_error)?;
        Self::apply_pragmas(&connection, &options)?;
        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn from_connection(connection: Connection) -> CoreResult<Self> {
        let options = SqliteOpenOptions::in_memory();
        Self::apply_pragmas(&connection, &options)?;
        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn apply_pragmas(connection: &Connection, options: &SqliteOpenOptions) -> CoreResult<()> {
        connection
            .query_row(
                &format!(
                    "PRAGMA journal_mode = {}",
                    options.journal_mode.as_pragma_value()
                ),
                [],
                |_row| Ok(()),
            )
            .optional()
            .map_err(sql_error)?;
        connection
            .query_row("PRAGMA synchronous = NORMAL", [], |_row| Ok(()))
            .optional()
            .map_err(sql_error)?;
        if let Some(timeout_ms) = options.busy_timeout_ms {
            connection
                .query_row(
                    &format!("PRAGMA busy_timeout = {}", timeout_ms),
                    [],
                    |_row| Ok(()),
                )
                .optional()
                .map_err(sql_error)?;
        }
        if options.foreign_keys {
            connection
                .query_row("PRAGMA foreign_keys = ON", [], |_row| Ok(()))
                .optional()
                .map_err(sql_error)?;
        }
        connection
            .query_row("PRAGMA cache_size = 64000", [], |_row| Ok(()))
            .optional()
            .map_err(sql_error)?;
        Ok(())
    }

    pub(crate) fn lock(&self) -> CoreResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| CoreError::Adapter {
            adapter: "engram-store-sqlite".to_owned(),
            message: "connection lock poisoned".to_owned(),
        })
    }

    fn write_procedure_row(&self, procedure: &Procedure) -> CoreResult<()> {
        let json = serde_json::to_string(procedure).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO procedures
                    (id, tenant, subject, workspace, session, environment, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(id) DO UPDATE SET
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    record_json = excluded.record_json
                "#,
                params![
                    procedure.id.to_string(),
                    procedure.scope.tenant,
                    procedure.scope.subject,
                    procedure.scope.workspace,
                    procedure.scope.session,
                    procedure.scope.environment,
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    fn load_all_procedures(&self) -> CoreResult<Vec<Procedure>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM procedures ORDER BY id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut out = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            out.push(serde_json::from_str::<Procedure>(&json).map_err(json_error)?);
        }
        Ok(out)
    }

    fn load_procedure_by_id(&self, id: &ProcedureId) -> CoreResult<Option<Procedure>> {
        let connection = self.lock()?;
        connection
            .query_row(
                "SELECT record_json FROM procedures WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<Procedure>(&json).map_err(json_error))
            .transpose()
    }

    fn visible_or_not_found(&self, id: &ProcedureId, scope: &Scope) -> CoreResult<Procedure> {
        self.load_procedure_by_id(id)?
            .filter(|p| scope_allows(&p.scope, scope))
            .ok_or_else(|| CoreError::NotFound {
                target_type: "procedure",
                target_id: id.to_string(),
            })
    }
}

#[async_trait]
impl ProcedureRepository for SqlProcedureStore {
    async fn upsert_procedure(&self, procedure: Procedure) -> CoreResult<Procedure> {
        self.write_procedure_row(&procedure)?;
        Ok(procedure)
    }

    async fn get_procedure(
        &self,
        id: &ProcedureId,
        scope: &Scope,
    ) -> CoreResult<Option<Procedure>> {
        Ok(self
            .load_procedure_by_id(id)?
            .filter(|p| scope_allows(&p.scope, scope)))
    }

    async fn get_procedure_by_name(
        &self,
        name: &str,
        scope: &Scope,
    ) -> CoreResult<Option<Procedure>> {
        Ok(self
            .load_all_procedures()?
            .into_iter()
            .filter(|p| scope_allows(&p.scope, scope))
            .find(|p| p.name == name))
    }

    async fn list_procedures(&self, scope: &Scope) -> CoreResult<Vec<Procedure>> {
        Ok(self
            .load_all_procedures()?
            .into_iter()
            .filter(|p| scope_allows(&p.scope, scope))
            .collect())
    }

    async fn increment_success(&self, id: &ProcedureId, scope: &Scope) -> CoreResult<Procedure> {
        let mut p = self.visible_or_not_found(id, scope)?;
        p.success_count = p.success_count.saturating_add(1);
        p.updated_at = Some(Utc::now());
        self.write_procedure_row(&p)?;
        Ok(p)
    }

    async fn increment_failure(&self, id: &ProcedureId, scope: &Scope) -> CoreResult<Procedure> {
        let mut p = self.visible_or_not_found(id, scope)?;
        p.failure_count = p.failure_count.saturating_add(1);
        p.updated_at = Some(Utc::now());
        self.write_procedure_row(&p)?;
        Ok(p)
    }

    async fn procedure_stats(&self, scope: &Scope) -> CoreResult<ProcedureStats> {
        let visible: Vec<_> = self
            .load_all_procedures()?
            .into_iter()
            .filter(|p| scope_allows(&p.scope, scope))
            .collect();
        Ok(ProcedureStats {
            total: visible.len(),
            total_success: visible.iter().map(|p| p.success_count as u64).sum(),
            total_failure: visible.iter().map(|p| p.failure_count as u64).sum(),
        })
    }

    async fn delete_procedure(&self, id: &ProcedureId, scope: &Scope) -> CoreResult<bool> {
        if self.visible_or_not_found(id, scope).is_err() {
            return Ok(false);
        }
        let connection = self.lock()?;
        let removed = connection
            .execute(
                "DELETE FROM procedures WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(sql_error)?;
        Ok(removed > 0)
    }
}
