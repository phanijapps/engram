//! SQLite implementation of the [`SourceBatchLedger`] port.
//!
//! One row per record a sourced batch wrote, keyed by a composite
//! `row_key` (scope discriminators + source_id + batch_key + record_kind +
//! record_id) so ledger re-records upsert idempotently — the same composite-key
//! shape the memory adapter uses for its write-idempotency table. The
//! supersession hot path reads through `idx_source_batch_ledger_source`
//! (tenant, source_id, superseded_at) — never a full-table scan — and every
//! query is scope-bounded. `record_id` values stay opaque strings (entity and
//! relationship ids verbatim; engine-assigned memory ids), never memory
//! internals.

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::Scope;
use engram_knowledge::{SourceBatchLedger, SourceBatchRecord, SourceBatchRecordKind};
use engram_runtime::CoreResult;

use crate::knowledge::schema::sql_error;
use crate::knowledge::service::SqlKnowledgeStore;

/// Composite row key: the scope discriminator columns + ledger identity. The
/// `\u{1f}` separator mirrors the memory idempotency key precedent.
fn row_key(scope: &Scope, source_id: &str, batch_key: &str, kind: &str, record_id: &str) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        scope.tenant,
        scope.subject.as_deref().unwrap_or_default(),
        scope.workspace.as_deref().unwrap_or_default(),
        scope.session.as_deref().unwrap_or_default(),
        scope.environment.as_deref().unwrap_or_default(),
        source_id,
        batch_key,
        kind,
        record_id,
    )
}

/// Stable storage name for a ledger record kind.
fn kind_name(kind: SourceBatchRecordKind) -> &'static str {
    match kind {
        SourceBatchRecordKind::Entity => "entity",
        SourceBatchRecordKind::Relationship => "relationship",
        SourceBatchRecordKind::MemoryRecord => "memory_record",
    }
}

fn parse_kind(raw: &str) -> SourceBatchRecordKind {
    match raw {
        "entity" => SourceBatchRecordKind::Entity,
        "relationship" => SourceBatchRecordKind::Relationship,
        _ => SourceBatchRecordKind::MemoryRecord,
    }
}

#[async_trait]
impl SourceBatchLedger for SqlKnowledgeStore {
    /// Lists the source's not-yet-superseded rows within `scope` (all batch
    /// keys), via the (tenant, source_id, superseded_at) index. Scope
    /// visibility mirrors the sibling knowledge reads: NULL record scope
    /// columns are wildcards, Some request fields require equality.
    async fn list_source_batch_records(
        &self,
        scope: &Scope,
        source_id: &str,
    ) -> CoreResult<Vec<SourceBatchRecord>> {
        let connection = self.lock()?;
        let mut stmt = connection
            .prepare(
                r#"
                SELECT batch_key, record_kind, record_id
                FROM source_batch_ledger
                WHERE tenant = ?1
                    AND (subject IS NULL OR subject = ?2)
                    AND (workspace IS NULL OR workspace = ?3)
                    AND source_id = ?4
                    AND superseded_at IS NULL
                ORDER BY batch_key, record_id
                "#,
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(
                rusqlite::params![scope.tenant, scope.subject, scope.workspace, source_id],
                |row| {
                    Ok(SourceBatchRecord {
                        source_id: source_id.to_owned(),
                        batch_key: row.get(0)?,
                        kind: parse_kind(&row.get::<_, String>(1)?),
                        record_id: row.get(2)?,
                        superseded_at: None,
                    })
                },
            )
            .map_err(sql_error)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(sql_error)?);
        }
        Ok(records)
    }

    /// Upserts one row per written record id. A re-record (same unique row
    /// key) refreshes `recorded_at` and clears `superseded_at` — a re-asserted
    /// record becomes the current batch's row again. Empty `record_ids` is a
    /// no-op.
    async fn record_source_batch_writes(
        &self,
        scope: &Scope,
        source_id: &str,
        batch_key: &str,
        kind: SourceBatchRecordKind,
        record_ids: &[String],
    ) -> CoreResult<()> {
        if record_ids.is_empty() {
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        let kind_str = kind_name(kind);
        let mut connection = self.lock()?;
        let tx = connection.transaction().map_err(sql_error)?;
        {
            let mut stmt = tx
                .prepare(
                    r#"
                    INSERT INTO source_batch_ledger
                        (row_key, tenant, subject, workspace, session, environment,
                         source_id, batch_key, record_kind, record_id, recorded_at, superseded_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL)
                    ON CONFLICT(row_key) DO UPDATE SET
                        recorded_at = excluded.recorded_at,
                        superseded_at = NULL
                    "#,
                )
                .map_err(sql_error)?;
            for record_id in record_ids {
                stmt.execute(rusqlite::params![
                    row_key(scope, source_id, batch_key, kind_str, record_id),
                    scope.tenant,
                    scope.subject,
                    scope.workspace,
                    scope.session,
                    scope.environment,
                    source_id,
                    batch_key,
                    kind_str,
                    record_id,
                    now,
                ])
                .map_err(sql_error)?;
            }
        }
        tx.commit().map_err(sql_error)?;
        Ok(())
    }

    /// Marks one ledger row superseded (idempotent overwrite of the stamp).
    async fn mark_source_batch_superseded(
        &self,
        scope: &Scope,
        source_id: &str,
        batch_key: &str,
        kind: SourceBatchRecordKind,
        record_id: &str,
    ) -> CoreResult<()> {
        let now = Utc::now().to_rfc3339();
        let connection = self.lock()?;
        connection
            .execute(
                "UPDATE source_batch_ledger SET superseded_at = ?1 WHERE row_key = ?2",
                rusqlite::params![
                    now,
                    row_key(scope, source_id, batch_key, kind_name(kind), record_id)
                ],
            )
            .map_err(sql_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! In-module ledger unit tests: upsert idempotency (re-record clears the
    //! superseded stamp), supersession filtering (superseded rows leave the
    //! list → second pass is a zero-count no-op), and scope bounding (a
    //! different tenant's rows are invisible).

    use super::*;
    use engram_domain::Scope;

    fn scope(tenant: &str) -> Scope {
        Scope {
            tenant: tenant.to_owned(),
            subject: Some("subject-ledger".to_owned()),
            workspace: Some("workspace-ledger".to_owned()),
            session: None,
            environment: None,
        }
    }

    #[test]
    fn ledger_record_mark_and_idempotent_re_record() {
        let store = SqlKnowledgeStore::open_in_memory().expect("open");
        let scope = scope("tenant-a");
        // Record two writes for batch-1.
        futures_block_on(store.record_source_batch_writes(
            &scope,
            "docs/x.md",
            "batch-1",
            SourceBatchRecordKind::Entity,
            &["e-1".to_owned(), "e-2".to_owned()],
        ))
        .expect("record");
        let rows =
            futures_block_on(store.list_source_batch_records(&scope, "docs/x.md")).expect("list");
        assert_eq!(rows.len(), 2, "both rows listed");

        // Mark e-1 superseded → only e-2 remains listed.
        futures_block_on(store.mark_source_batch_superseded(
            &scope,
            "docs/x.md",
            "batch-1",
            SourceBatchRecordKind::Entity,
            "e-1",
        ))
        .expect("mark");
        let rows =
            futures_block_on(store.list_source_batch_records(&scope, "docs/x.md")).expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].record_id, "e-2");
        assert_eq!(rows[0].batch_key, "batch-1");

        // Re-record the same (source, batch, kind, record) id — upsert, not a
        // duplicate row, and the superseded stamp clears.
        futures_block_on(store.record_source_batch_writes(
            &scope,
            "docs/x.md",
            "batch-1",
            SourceBatchRecordKind::Entity,
            &["e-1".to_owned()],
        ))
        .expect("re-record");
        let rows =
            futures_block_on(store.list_source_batch_records(&scope, "docs/x.md")).expect("list");
        assert_eq!(rows.len(), 2, "re-record upserted, no duplicate rows");

        // Unknown source lists empty.
        let none = futures_block_on(store.list_source_batch_records(&scope, "docs/other.md"))
            .expect("list");
        assert!(none.is_empty());
    }

    #[test]
    fn ledger_rows_are_scope_bounded() {
        let store = SqlKnowledgeStore::open_in_memory().expect("open");
        let scope_a = scope("tenant-a");
        let scope_b = scope("tenant-b");
        futures_block_on(store.record_source_batch_writes(
            &scope_a,
            "docs/x.md",
            "batch-1",
            SourceBatchRecordKind::MemoryRecord,
            &["memory-1".to_owned()],
        ))
        .expect("record in A");
        // Scope B sees nothing — not even the same source id.
        let b_rows = futures_block_on(store.list_source_batch_records(&scope_b, "docs/x.md"))
            .expect("list B");
        assert!(b_rows.is_empty(), "cross-tenant ledger read must be empty");
        // Marking in scope B affects no rows.
        futures_block_on(store.mark_source_batch_superseded(
            &scope_b,
            "docs/x.md",
            "batch-1",
            SourceBatchRecordKind::MemoryRecord,
            "memory-1",
        ))
        .expect("mark in B");
        let a_rows = futures_block_on(store.list_source_batch_records(&scope_a, "docs/x.md"))
            .expect("list A");
        assert_eq!(a_rows.len(), 1, "A's row untouched by B's mark");
    }

    /// Drives a future on the current thread without an async runtime.
    fn futures_block_on<F: std::future::Future>(f: F) -> F::Output {
        use std::task::{Context, Poll, Waker};
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut fut = std::pin::pin!(f);
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(out) => return out,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }
}
