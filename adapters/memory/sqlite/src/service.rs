//! SQLite-backed repository implementation.
//!
//! This module implements storage ports only. Operation orchestration such as
//! write validation, policy gates, retrieval ranking, and forget behavior stays
//! in service-level modules or reusable conformance runners.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use engram_domain::*;
use engram_memory::{CoreError, CoreResult, MemoryEventRepository, MemoryRepository};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    schema::{initialize_schema, json_error, sql_error},
    scope::scope_allows,
};

/// SQLite-backed memory repository.
///
/// The store preserves `MemoryRecord` and `MemoryEvent` payloads as contract
/// JSON while indexing identifiers and scope columns for repository lookups.
#[derive(Clone)]
pub struct SqlMemoryStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqlMemoryStore {
    /// Opens an in-memory SQLite store and initializes the Engram schema.
    ///
    /// This constructor is intended for conformance tests and local examples.
    /// File-backed constructors can be added without changing repository
    /// behavior.
    pub fn open_in_memory() -> CoreResult<Self> {
        let connection = Connection::open_in_memory().map_err(sql_error)?;
        Self::from_connection(connection)
    }

    /// Opens a file-backed SQLite store and initializes the Engram schema.
    ///
    /// This constructor is intended for local durable smoke tests and embedded
    /// development workflows. It preserves the same repository behavior as the
    /// in-memory constructor; the file path remains adapter configuration, not
    /// portable memory data.
    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        let connection = Connection::open(path).map_err(sql_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> CoreResult<Self> {
        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Locks the SQLite connection and maps synchronization failure to core.
    ///
    /// The adapter uses one connection behind a mutex for deterministic
    /// conformance tests. Future pooled adapters should preserve the same error
    /// boundary.
    pub(crate) fn lock_connection(&self) -> CoreResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| CoreError::Adapter {
            adapter: "engram-store-sql".to_owned(),
            message: "connection lock poisoned".to_owned(),
        })
    }

    /// Lists all memories for service-level candidate scanning.
    ///
    /// Retrieval still applies scope and policy after loading records; this
    /// helper exists so the SQL service can share deterministic baseline
    /// behavior with the in-memory adapter before specialized indexes exist.
    pub(crate) fn list_memories(&self) -> CoreResult<Vec<MemoryRecord>> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM memories")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut records = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            records.push(serde_json::from_str::<MemoryRecord>(&json).map_err(json_error)?);
        }
        Ok(records)
    }

    /// Removes a scoped memory record for hard-delete behavior.
    ///
    /// The method first verifies the stored record is visible to the supplied
    /// scope so cross-tenant delete attempts cannot remove hidden data.
    pub(crate) fn remove_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<bool> {
        let connection = self.lock_connection()?;
        let record = connection
            .query_row(
                "SELECT record_json FROM memories WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<MemoryRecord>(&json).map_err(json_error))
            .transpose()?;
        let Some(record) = record.filter(|record| scope_allows(&record.scope, scope)) else {
            return Ok(false);
        };
        let deleted = connection
            .execute(
                "DELETE FROM memories WHERE id = ?1",
                params![record.id.to_string()],
            )
            .map_err(sql_error)?;
        Ok(deleted > 0)
    }

    /// Returns a stored idempotent write response when the scoped key exists.
    ///
    /// Service write orchestration uses this to return the original response
    /// without appending a duplicate lifecycle event.
    pub(crate) fn get_idempotent_response(
        &self,
        key: &str,
    ) -> CoreResult<Option<WriteMemoryResponse>> {
        let connection = self.lock_connection()?;
        connection
            .query_row(
                "SELECT response_json FROM write_idempotency WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<WriteMemoryResponse>(&json).map_err(json_error))
            .transpose()
    }
}

#[async_trait]
impl MemoryRepository for SqlMemoryStore {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        let json = serde_json::to_string(&record).map_err(json_error)?;
        let connection = self.lock_connection()?;
        connection
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
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(record)
    }

    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        let connection = self.lock_connection()?;
        let record = connection
            .query_row(
                "SELECT record_json FROM memories WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<MemoryRecord>(&json).map_err(json_error))
            .transpose()?;
        Ok(record.filter(|record| scope_allows(&record.scope, scope)))
    }

    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent> {
        let json = serde_json::to_string(&event).map_err(json_error)?;
        let connection = self.lock_connection()?;
        connection
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
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(event)
    }

    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        let mut record = self
            .get_memory(id, scope)
            .await?
            .ok_or_else(|| CoreError::NotFound {
                target_type: "memory",
                target_id: id.to_string(),
            })?;
        record.status = status;
        self.put_memory(record).await
    }
}

#[async_trait]
impl MemoryEventRepository for SqlMemoryStore {
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        let connection = self.lock_connection()?;
        let event = connection
            .query_row(
                "SELECT event_json FROM memory_events WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<MemoryEvent>(&json).map_err(json_error))
            .transpose()?;
        Ok(event.filter(|event| scope_allows(&event.scope, scope)))
    }

    async fn list_events_for_memory(
        &self,
        memory_id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT event_json
                FROM memory_events
                WHERE memory_id = ?1
                ORDER BY sequence ASC
                "#,
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![memory_id.to_string()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        let mut events = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let event = serde_json::from_str::<MemoryEvent>(&json).map_err(json_error)?;
            if scope_allows(&event.scope, scope) {
                events.push(event);
            }
        }
        Ok(events)
    }

    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare("SELECT event_json FROM memory_events ORDER BY sequence ASC")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut events = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let event = serde_json::from_str::<MemoryEvent>(&json).map_err(json_error)?;
            if scope_allows(&event.scope, scope) {
                events.push(event);
            }
        }
        Ok(events)
    }
}
