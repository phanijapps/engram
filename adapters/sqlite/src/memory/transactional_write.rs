//! Atomic SQL write transaction for memory creation.
//!
//! This module owns the SQLite transaction that keeps a write, its lifecycle
//! event, and its optional idempotency record from being partially committed.

use async_trait::async_trait;
use engram_domain::*;
use engram_memory::{CoreError, CoreResult, MemoryStorage};
use engram_runtime::effect::{
    CleanupCertainty, EffectIdentity, EffectObservation, EffectOutcome, ObservedResult,
};
use rusqlite::{OptionalExtension, params};

use crate::memory::{
    schema::{json_error, sql_error},
    service::SqlMemoryStore,
};

/// Persists a memory write, event, and optional idempotency response atomically.
///
/// The caller performs validation, authorization, ID generation, and event
/// construction. This function owns only the SQL durability boundary so
/// concurrent idempotent writes cannot create duplicate records or events.
pub(crate) fn write_memory_transaction(
    store: &SqlMemoryStore,
    idempotency_key: Option<&str>,
    record: MemoryRecord,
    event: MemoryEvent,
) -> CoreResult<WriteMemoryResponse> {
    if record.scope != event.scope
        || event.memory_id.as_ref() != Some(&record.id)
        || event.kind != MemoryEventKind::Written
    {
        return Err(CoreError::Conflict {
            reason: "memory write record/event identity mismatch".into(),
        });
    }
    let scope = record.scope.clone();
    let scoped_key = idempotency_key
        .map(|key| scoped_key_for(&scope, key))
        .transpose()?;
    let mut connection = store.lock_connection()?;
    let transaction = connection.transaction().map_err(sql_error)?;

    if let Some(key) = idempotency_key {
        if let Some(mut response) = lookup_response(&transaction, &scope, key)? {
            response.deduplicated = Some(true);
            transaction.rollback().map_err(sql_error)?;
            return Ok(response);
        }
    }

    let record_json = serde_json::to_string(&record).map_err(json_error)?;
    let event_json = serde_json::to_string(&event).map_err(json_error)?;
    transaction
        .execute(
            r#"
            INSERT INTO memories
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
                record.id.to_string(),
                record.scope.tenant,
                record.scope.subject,
                record.scope.workspace,
                record.scope.session,
                record.scope.environment,
                record_json
            ],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            r#"
            INSERT INTO memory_events
                (id, memory_id, tenant, subject, workspace, session, environment, event_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                event.id.to_string(),
                event.memory_id.as_ref().map(ToString::to_string),
                event.scope.tenant,
                event.scope.subject,
                event.scope.workspace,
                event.scope.session,
                event.scope.environment,
                event_json
            ],
        )
        .map_err(sql_error)?;

    let response = WriteMemoryResponse {
        record,
        event,
        deduplicated: Some(false),
    };

    if let Some(key) = scoped_key.as_deref() {
        let response_json = serde_json::to_string(&response).map_err(json_error)?;
        let inserted = transaction
            .execute(
                r#"
                INSERT INTO write_idempotency (key, response_json)
                VALUES (?1, ?2)
                ON CONFLICT(key) DO NOTHING
                "#,
                params![key, response_json],
            )
            .map_err(sql_error)?;
        if inserted == 0 {
            transaction.rollback().map_err(sql_error)?;
            drop(connection);
            let connection = store.lock_connection()?;
            let Some(mut existing) =
                lookup_response(&connection, &scope, idempotency_key.expect("key present"))?
            else {
                return Err(crate::memory::schema::sql_error(
                    rusqlite::Error::QueryReturnedNoRows,
                ));
            };
            existing.deduplicated = Some(true);
            return Ok(existing);
        }
    }

    transaction.commit().map_err(sql_error)?;
    Ok(response)
}

/// JSON tuple encoding preserves unset dimensions and arbitrary opaque user keys.
fn scoped_key_for(scope: &Scope, key: &str) -> CoreResult<String> {
    serde_json::to_string(&(scope, key))
        .map(|key| format!("scope-v2:{key}"))
        .map_err(json_error)
}
fn legacy_key_for(scope: &Scope, key: &str) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        scope.tenant,
        scope.subject.as_deref().unwrap_or_default(),
        scope.workspace.as_deref().unwrap_or_default(),
        key
    )
}
fn lookup_response(
    connection: &rusqlite::Connection,
    scope: &Scope,
    key: &str,
) -> CoreResult<Option<WriteMemoryResponse>> {
    for (storage_key, legacy) in [
        (scoped_key_for(scope, key)?, false),
        (legacy_key_for(scope, key), true),
    ] {
        let json = connection
            .query_row(
                "SELECT response_json FROM write_idempotency WHERE key = ?1",
                params![storage_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?;
        if let Some(json) = json {
            let response: WriteMemoryResponse = serde_json::from_str(&json).map_err(json_error)?;
            if response.record.scope == *scope
                && response.event.scope == *scope
                && response.event.memory_id.as_ref() == Some(&response.record.id)
                && response.event.kind == MemoryEventKind::Written
            {
                return Ok(Some(response));
            }
            if !legacy {
                return Err(CoreError::Conflict {
                    reason: "idempotent response scope mismatch".into(),
                });
            }
            // Old keys omitted session/environment and conflated absent/empty values.
            // Never reuse a sibling's response; the full tuple has its own new key.
        }
    }
    Ok(None)
}

#[async_trait]
impl MemoryStorage for SqlMemoryStore {
    async fn enumerate(&self, scope: &Scope) -> CoreResult<Vec<MemoryRecord>> {
        let mut records = self.list_memories()?;
        records.retain(|record| super::scope::scope_allows(&record.scope, scope));
        Ok(records)
    }
    async fn idempotent_response(
        &self,
        scope: &Scope,
        key: &str,
    ) -> CoreResult<Option<WriteMemoryResponse>> {
        let connection = self.lock_connection()?;
        lookup_response(&connection, scope, key)
    }
    async fn commit_write(
        &self,
        key: Option<&str>,
        record: MemoryRecord,
        event: MemoryEvent,
    ) -> ObservedResult<WriteMemoryResponse> {
        static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let token = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let identity = EffectIdentity {
            actor: event.actor.id.to_string(),
            call: format!("sqlite-call-{token}"),
            session: format!("sqlite-operation-session-{token}"),
            backend: "sqlite".into(),
            mode: "local".into(),
            store: match self.lock_connection() {
                Ok(connection) => format!("{:p}", &*connection),
                Err(_) => "unavailable".into(),
            },
            scope: record.scope.clone(),
            item: record.id.to_string(),
        };
        let result = write_memory_transaction(self, key, record, event);
        let outcome = if result.is_ok() {
            EffectOutcome::Acknowledged
        } else {
            EffectOutcome::DefiniteFailure
        };
        ObservedResult {
            result,
            observation: EffectObservation {
                identity,
                step: Some("memory.write".into()),
                commit_id: None,
                outcome,
                cleanup: CleanupCertainty::Confirmed,
            },
        }
    }
    async fn remove(&self, id: &MemoryId, scope: &Scope) -> CoreResult<bool> {
        self.remove_memory(id, scope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::SqlMemoryService;
    use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryService};
    use futures::executor::block_on;
    use serde_json::json;

    fn request() -> WriteMemoryRequest {
        serde_json::from_value(json!({
            "kind":"observation", "content":{"text":"atomic anchor"},
            "scope":{"tenant":"tenant","subject":"subject","workspace":"workspace","session":"one","environment":"test"},
            "requester":{"actor":{"id":"actor","kind":"agent"}},
            "provenance":{"source":"fixture","actor":{"id":"actor","kind":"agent"},"observedAt":"2026-11-01T12:00:00Z"},
            "policy":{"visibility":"private","retention":"durable","allowedUses":["retrieval"]},
            "idempotencyKey":"opaque\u{1f}key"
        })).unwrap()
    }

    #[test]
    fn sqlite_failed_event_insert_rolls_back_record_and_idempotency_response() {
        let service = SqlMemoryService::open_in_memory().unwrap();
        service.store.lock_connection().unwrap().execute_batch(
            "CREATE TRIGGER reject_event BEFORE INSERT ON memory_events BEGIN SELECT RAISE(ABORT, 'contract failure'); END;"
        ).unwrap();
        let request = request();
        assert!(block_on(service.write_memory(request.clone())).is_err());
        assert!(service.store.list_memories().unwrap().is_empty());
        assert!(
            block_on(service.list_events_for_scope(&request.scope))
                .unwrap()
                .is_empty()
        );
        assert!(
            block_on(
                service.store.idempotent_response(
                    &request.scope,
                    request.idempotency_key.as_deref().unwrap()
                )
            )
            .unwrap()
            .is_none()
        );
        service
            .store
            .lock_connection()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_event")
            .unwrap();
        let first = block_on(service.write_memory(request.clone())).unwrap();
        let retry = block_on(service.write_memory(request.clone())).unwrap();
        assert_eq!(first.record, retry.record);
        assert_eq!(first.event, retry.event);
        assert_eq!(retry.deduplicated, Some(true));
        assert_eq!(
            block_on(service.list_events_for_scope(&request.scope))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn sqlite_legacy_retry_validates_every_scope_dimension_and_preserves_response() {
        let service = SqlMemoryService::open_in_memory().unwrap();
        let mut request = request();
        let first = block_on(service.write_memory(request.clone())).unwrap();
        {
            let connection = service.store.lock_connection().unwrap();
            connection
                .execute("DELETE FROM write_idempotency", [])
                .unwrap();
            connection
                .execute(
                    "INSERT INTO write_idempotency(key,response_json) VALUES(?1,?2)",
                    params![
                        legacy_key_for(&request.scope, request.idempotency_key.as_deref().unwrap()),
                        serde_json::to_string(&first).unwrap()
                    ],
                )
                .unwrap();
        }
        request.content.text = "retry replacement must not persist".into();
        let retry = block_on(service.write_memory(request.clone())).unwrap();
        assert_eq!(retry.record, first.record);
        assert_eq!(retry.event, first.event);
        assert_eq!(retry.deduplicated, Some(true));
        for dimension in ["subject", "session", "environment"] {
            let mut changed = request.clone();
            match dimension {
                "subject" => changed.scope.subject = Some("two".into()),
                "session" => changed.scope.session = Some("two".into()),
                _ => changed.scope.environment = Some("two".into()),
            }
            let written = block_on(service.write_memory(changed.clone())).unwrap();
            assert_ne!(written.record.id, first.record.id);
            assert_eq!(written.record.scope, changed.scope);
            assert_eq!(written.deduplicated, Some(false));
            assert_eq!(
                block_on(service.write_memory(changed.clone()))
                    .unwrap()
                    .record
                    .id,
                written.record.id
            );
            assert_eq!(
                block_on(service.list_events_for_memory(&written.record.id, &changed.scope))
                    .unwrap()
                    .len(),
                1
            );
        }
        assert_eq!(service.store.list_memories().unwrap().len(), 4);
    }

    #[test]
    fn sqlite_delegate_reopen_keeps_idempotency_and_delete_keeps_audit_events() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("memory.db");
        let request = request();
        let first = {
            let service = SqlMemoryService::open_file(&path).unwrap();
            block_on(service.write_memory(request.clone())).unwrap()
        };
        let service = SqlMemoryService::open_file(&path).unwrap();
        let retry = block_on(service.write_memory(request.clone())).unwrap();
        assert_eq!(retry.record, first.record);
        let result = block_on(service.forget(ForgetRequest {
            target_type: ForgetTargetType::Memory,
            target_id: first.record.id.to_string(),
            scope: request.scope.clone(),
            requester: request.requester,
            mode: DeleteMode::Delete,
            reason: None,
        }))
        .unwrap();
        assert_eq!(result.status, ForgetStatus::Deleted);
        assert!(
            block_on(service.get_memory(&first.record.id, &request.scope))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            block_on(service.list_events_for_memory(&first.record.id, &request.scope))
                .unwrap()
                .len(),
            2
        );
    }
}
