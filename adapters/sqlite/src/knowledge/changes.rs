//! Knowledge change log + scan checkpoints (enhancements adoption, slice 3).
//!
//! The log is invalidation metadata: mutations append rows under the SAME
//! connection lock as the mutation itself (writer-atomic; a crash between
//! the mutation and the append can under-report — consumers reconcile from
//! checkpoints, documented in the spec). No bodies, no secrets. Sequence
//! numbers are rowids of a dedicated log table, serialized as decimal
//! strings on JSON surfaces.

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::{
    CodeGraphStaleness, KnowledgeChange, KnowledgeChangeOp, KnowledgeChangeTarget,
    KnowledgeChangesPage, KnowledgeChangesRequest, ScanCheckpoint, Scope,
};
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};

use crate::knowledge::schema::{json_error, sql_error};
use crate::knowledge::scope::scope_allows;
use crate::knowledge::service::SqlKnowledgeStore;

/// Creates the change-log + checkpoint tables (idempotent).
pub(crate) fn initialize_schema(connection: &Connection) -> CoreResult<()> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS knowledge_change_log (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                stable_source_key TEXT,
                graph_id TEXT,
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                op TEXT NOT NULL,
                occurred_at TEXT NOT NULL,
                scan_run_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_knowledge_change_log_scope
                ON knowledge_change_log (tenant, subject, workspace, session, environment, seq);
            CREATE INDEX IF NOT EXISTS idx_knowledge_change_log_source
                ON knowledge_change_log (tenant, stable_source_key, seq);
            CREATE TABLE IF NOT EXISTS scan_checkpoints (
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                stable_source_key TEXT NOT NULL,
                started_at TEXT NOT NULL,
                record_json TEXT NOT NULL,
                PRIMARY KEY (tenant, subject, workspace, session, environment, stable_source_key)
            );
            "#,
        )
        .map_err(sql_error)
}

/// Appends one change row while the caller holds the store's connection
/// lock (the mutation and its log entry commit under one writer).
#[allow(clippy::too_many_arguments)]
pub(crate) fn append_change_locked(
    connection: &Connection,
    scope: &Scope,
    graph_id: Option<&str>,
    target_type: KnowledgeChangeTarget,
    target_id: &str,
    op: KnowledgeChangeOp,
) -> CoreResult<()> {
    connection
        .execute(
            "INSERT INTO knowledge_change_log (tenant, subject, workspace, session, environment, graph_id, target_type, target_id, op, occurred_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params!(
                scope.tenant,
                scope.subject,
                scope.workspace,
                scope.session,
                scope.environment,
                graph_id,
                target_type.as_str(),
                target_id,
                op.as_str(),
                Utc::now().to_rfc3339(),
            ),
        )
        .map(|_| ())
        .map_err(sql_error)
}

/// Returns the stored `record_json` for a row, when present (no-op detection).
pub(crate) fn stored_json(
    connection: &Connection,
    table: &str,
    id: &str,
) -> CoreResult<Option<String>> {
    connection
        .query_row(
            &format!("SELECT record_json FROM {table} WHERE id = ?1"),
            [id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)
}

fn change_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeChange> {
    Ok(KnowledgeChange {
        seq: row.get::<_, i64>("seq")?.to_string(),
        scope: Scope {
            tenant: row.get("tenant")?,
            subject: row.get("subject")?,
            workspace: row.get("workspace")?,
            session: row.get("session")?,
            environment: row.get("environment")?,
        },
        stable_source_key: row.get("stable_source_key")?,
        graph_id: row
            .get::<_, Option<String>>("graph_id")?
            .map(engram_domain::Id::from),
        target_type: match row.get::<_, String>("target_type")?.as_str() {
            "graph" => KnowledgeChangeTarget::Graph,
            "entity" => KnowledgeChangeTarget::Entity,
            "relationship" => KnowledgeChangeTarget::Relationship,
            "document" => KnowledgeChangeTarget::Document,
            _ => KnowledgeChangeTarget::Chunk,
        },
        target_id: engram_domain::Id::from(row.get::<_, String>("target_id")?),
        op: match row.get::<_, String>("op")?.as_str() {
            "remove" => KnowledgeChangeOp::Remove,
            "archive" => KnowledgeChangeOp::Archive,
            "restore" => KnowledgeChangeOp::Restore,
            _ => KnowledgeChangeOp::Upsert,
        },
        occurred_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>("occurred_at")?)
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        scan_run_id: row.get("scan_run_id")?,
    })
}

#[async_trait]
impl engram_knowledge::KnowledgeChangeLog for SqlKnowledgeStore {
    async fn changes_since(
        &self,
        request: &KnowledgeChangesRequest,
    ) -> CoreResult<KnowledgeChangesPage> {
        let connection = self.lock_conn()?;
        let limit = request.limit.clamp(1, 500) as i64;
        let after: Option<i64> = match &request.after {
            Some(cursor) => Some(
                cursor
                    .parse::<i64>()
                    .map_err(|_| CoreError::InvalidRequest {
                        reason: format!("opaque cursor rejected: {cursor}"),
                    })?,
            ),
            None => None,
        };

        let mut sql = "SELECT seq, tenant, subject, workspace, session, environment, stable_source_key, graph_id, target_type, target_id, op, occurred_at, scan_run_id FROM knowledge_change_log WHERE tenant = ?1".to_owned();
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(request.scope.tenant.clone())];
        let mut idx = 2;
        for (col, value) in [
            ("subject", &request.scope.subject),
            ("workspace", &request.scope.workspace),
            ("session", &request.scope.session),
            ("environment", &request.scope.environment),
        ] {
            if let Some(v) = value {
                sql.push_str(&format!(" AND {col} IS ?{idx}"));
                binds.push(Box::new(v.clone()));
                idx += 1;
            }
        }
        if let Some(key) = &request.source_key {
            sql.push_str(&format!(" AND stable_source_key = ?{idx}"));
            binds.push(Box::new(key.clone()));
            idx += 1;
        }
        if let Some(seq) = after {
            sql.push_str(&format!(" AND seq > ?{idx}"));
            binds.push(Box::new(seq));
            idx += 1;
        }
        // fetch limit+1 to detect has_more without a second query
        sql.push_str(&format!(" ORDER BY seq ASC LIMIT ?{idx}"));
        binds.push(Box::new(limit + 1));

        let mut stmt = connection.prepare(&sql).map_err(sql_error)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(binds.iter()), change_from_row)
            .map_err(sql_error)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(sql_error)?);
        }
        let has_more = items.len() as i64 > limit;
        if has_more {
            items.truncate(limit as usize);
        }
        let next_cursor = items.last().map(|c| c.seq.clone());
        let high_watermark: Option<String> = connection
            .query_row(
                "SELECT MAX(seq) FROM knowledge_change_log WHERE tenant = ?1",
                [&request.scope.tenant],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()
            .map_err(sql_error)?
            .flatten()
            .map(|seq| seq.to_string());

        // Reset detection: a numeric cursor older than the retained window.
        let mut reset_required = false;
        if let (Some(seq), Some(_hw)) = (after, high_watermark.as_ref()) {
            let min_retained: Option<i64> = connection
                .query_row(
                    "SELECT MIN(seq) FROM knowledge_change_log WHERE tenant = ?1",
                    [&request.scope.tenant],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()
                .map_err(sql_error)?
                .flatten();
            if let Some(min) = min_retained {
                reset_required = seq + 1 < min;
            }
        }
        Ok(KnowledgeChangesPage {
            items,
            next_cursor: if has_more { next_cursor } else { None },
            high_watermark,
            has_more,
            reset_required,
        })
    }

    async fn latest_checkpoint(
        &self,
        scope: &Scope,
        stable_source_key: &str,
    ) -> CoreResult<Option<ScanCheckpoint>> {
        let connection = self.lock_conn()?;
        let json: Option<String> = connection
            .query_row(
                "SELECT record_json FROM scan_checkpoints WHERE tenant = ?1 AND stable_source_key = ?2",
                params![scope.tenant, stable_source_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let checkpoint: Option<ScanCheckpoint> = json
            .map(|j| serde_json::from_str(&j).map_err(json_error))
            .transpose()?;
        Ok(checkpoint.filter(|cp| scope_allows(&cp.scope, scope)))
    }

    async fn latest_checkpoints(&self, scope: &Scope) -> CoreResult<Vec<ScanCheckpoint>> {
        let connection = self.lock_conn()?;
        let mut stmt = connection
            .prepare(
                "SELECT record_json FROM scan_checkpoints WHERE tenant = ?1 ORDER BY started_at DESC",
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(params![scope.tenant], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut out = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let checkpoint: ScanCheckpoint = serde_json::from_str(&json).map_err(json_error)?;
            if scope_allows(&checkpoint.scope, scope) {
                out.push(checkpoint);
            }
        }
        Ok(out)
    }
}

#[async_trait]
impl engram_knowledge::ScanCheckpointStore for SqlKnowledgeStore {
    async fn put_checkpoint(&self, checkpoint: ScanCheckpoint) -> CoreResult<ScanCheckpoint> {
        let connection = self.lock_conn()?;
        let json = serde_json::to_string(&checkpoint).map_err(json_error)?;
        // A stale run never overwrites a newer one (guard on started_at).
        connection
            .execute(
                "INSERT INTO scan_checkpoints (tenant, subject, workspace, session, environment, stable_source_key, started_at, record_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(tenant, subject, workspace, session, environment, stable_source_key) DO UPDATE SET
                    started_at = excluded.started_at, record_json = excluded.record_json
                 WHERE excluded.started_at >= scan_checkpoints.started_at",
                params!(
                    checkpoint.scope.tenant,
                    checkpoint.scope.subject,
                    checkpoint.scope.workspace,
                    checkpoint.scope.session,
                    checkpoint.scope.environment,
                    checkpoint.stable_source_key,
                    checkpoint.started_at.to_rfc3339(),
                    json,
                ),
            )
            .map_err(sql_error)?;
        Ok(checkpoint)
    }
}

impl SqlKnowledgeStore {
    /// Pure staleness derivation (delegates to the domain function).
    pub fn derive_staleness(
        checkpoint: Option<&ScanCheckpoint>,
        source_key: &str,
        current_revision: Option<&str>,
        commits_behind: Option<u64>,
        dirty: Option<bool>,
    ) -> CodeGraphStaleness {
        engram_domain::derive_staleness(
            checkpoint,
            source_key,
            current_revision,
            commits_behind,
            dirty,
        )
    }

    /// Convenience: same as [`Self::derive_staleness`] over the persisted
    /// checkpoint (async, store-backed).
    pub async fn staleness_for(
        &self,
        scope: &Scope,
        source_key: &str,
        current_revision: Option<&str>,
        commits_behind: Option<u64>,
        dirty: Option<bool>,
    ) -> CoreResult<CodeGraphStaleness> {
        let checkpoint = <Self as engram_knowledge::KnowledgeChangeLog>::latest_checkpoint(
            self, scope, source_key,
        )
        .await?;
        Ok(Self::derive_staleness(
            checkpoint.as_ref(),
            source_key,
            current_revision,
            commits_behind,
            dirty,
        ))
    }
}
