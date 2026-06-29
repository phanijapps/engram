//! Atomic SQL write transaction for memory creation.
//!
//! This module owns the SQLite transaction that keeps a write, its lifecycle
//! event, and its optional idempotency record from being partially committed.

use engram_core::CoreResult;
use engram_domain::*;
use rusqlite::{OptionalExtension, params};

use crate::{
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
    let mut connection = store.lock_connection()?;
    let transaction = connection.transaction().map_err(sql_error)?;

    if let Some(key) = idempotency_key
        && let Some(existing) = transaction
            .query_row(
                "SELECT response_json FROM write_idempotency WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<WriteMemoryResponse>(&json).map_err(json_error))
            .transpose()?
    {
        let mut response = existing;
        response.deduplicated = Some(true);
        transaction.rollback().map_err(sql_error)?;
        return Ok(response);
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

    if let Some(key) = idempotency_key {
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
            let Some(mut existing) = store.get_idempotent_response(key)? else {
                return Err(crate::schema::sql_error(
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
