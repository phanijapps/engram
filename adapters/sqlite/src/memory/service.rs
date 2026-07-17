//! SQLite-backed repository implementation.
//!
//! This module implements storage ports only. Operation orchestration such as
//! write validation, policy gates, retrieval ranking, and forget behavior stays
//! in service-level modules or reusable conformance runners.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use crate::{SqliteOpenOptions, SqlitePath};
use async_trait::async_trait;
use engram_domain::*;
use engram_memory::{CoreError, CoreResult, MemoryEventRepository, MemoryRepository};
use rusqlite::{Connection, OptionalExtension, params};

use crate::memory::{
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

    /// Opens a SQLite store with explicit configuration options.
    ///
    /// This constructor allows hosts like AgentZero's adapter to control WAL mode,
    /// busy timeout, foreign keys, migrations, and directory creation explicitly.
    ///
    /// # Arguments
    ///
    /// * `options` - SQLite configuration options including path, journal mode, and
    ///   pragma settings
    ///
    /// # Returns
    ///
    /// Returns a configured `SqlMemoryStore` instance with schema initialized.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crate::{SqliteOpenOptions, SqliteJournalMode, SqlitePath};
    /// use engram_store_sqlite::SqlMemoryStore;
    ///
    /// let options = SqliteOpenOptions {
    ///     path: SqlitePath::File("/path/to/engram.db".into()),
    ///     create_parent_dirs: true,
    ///     journal_mode: SqliteJournalMode::Wal,
    ///     busy_timeout_ms: Some(5000),
    ///     foreign_keys: true,
    ///     run_migrations: true,
    /// };
    ///
    /// let store = SqlMemoryStore::open_with_options(options)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn open_with_options(options: SqliteOpenOptions) -> CoreResult<Self> {
        let connection = match &options.path {
            SqlitePath::File(path) => {
                if options.create_parent_dirs {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| CoreError::Adapter {
                            adapter: "engram-store-sql".to_owned(),
                            message: format!("failed to create parent directory: {}", e),
                        })?;
                    }
                }
                Connection::open(path)
            }
            SqlitePath::InMemory => Connection::open_in_memory(),
        }
        .map_err(sql_error)?;

        // Apply pragmas from options
        Self::apply_pragmas(&connection, &options)?;

        // Initialize schema
        initialize_schema(&connection)?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn from_connection(connection: Connection) -> CoreResult<Self> {
        // Apply default pragmas for backward compatibility
        let options = SqliteOpenOptions::in_memory();
        Self::apply_pragmas(&connection, &options)?;

        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Applies SQLite PRAGMAs based on configuration options.
    ///
    /// This centralizes pragma application so both `open_with_options` and legacy
    /// constructors can use the same logic.
    fn apply_pragmas(connection: &Connection, options: &SqliteOpenOptions) -> CoreResult<()> {
        // Journal mode (returns result set, so we use query_row and ignore the result)
        let journal_pragma = format!(
            "PRAGMA journal_mode = {}",
            options.journal_mode.as_pragma_value()
        );
        connection
            .query_row(&journal_pragma, [], |_row| Ok(()))
            .optional()
            .map_err(sql_error)?;

        // Synchronous mode (NORMAL for WAL mode)
        connection
            .query_row("PRAGMA synchronous = NORMAL", [], |_row| Ok(()))
            .optional()
            .map_err(sql_error)?;

        // Busy timeout
        if let Some(timeout_ms) = options.busy_timeout_ms {
            let timeout_pragma = format!("PRAGMA busy_timeout = {}", timeout_ms);
            connection
                .query_row(&timeout_pragma, [], |_row| Ok(()))
                .optional()
                .map_err(sql_error)?;
        }

        // Foreign keys
        if options.foreign_keys {
            connection
                .query_row("PRAGMA foreign_keys = ON", [], |_row| Ok(()))
                .optional()
                .map_err(sql_error)?;
        }

        // Cache size (64MB default for better performance)
        connection
            .query_row("PRAGMA cache_size = 64000", [], |_row| Ok(()))
            .optional()
            .map_err(sql_error)?;

        Ok(())
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

    /// Returns the highest numeric suffix among existing memory and event IDs.
    ///
    /// Generated IDs have the form `<entity>-<n>`; reopening a file-backed
    /// database seeds the local ID generator past this value so new writes never
    /// collide with rows a previous process already wrote. Returns `0` when the
    /// database holds no parseable IDs.
    pub(crate) fn max_used_id_suffix(&self) -> CoreResult<u64> {
        let connection = self.lock_connection()?;
        let mut max_suffix = 0u64;
        for sql in ["SELECT id FROM memories", "SELECT id FROM memory_events"] {
            let mut statement = connection.prepare(sql).map_err(sql_error)?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(sql_error)?;
            for row in rows {
                let id = row.map_err(sql_error)?;
                if let Some(suffix) = id
                    .rsplit_once('-')
                    .and_then(|(_, digits)| digits.parse::<u64>().ok())
                {
                    max_suffix = max_suffix.max(suffix);
                }
            }
        }
        Ok(max_suffix)
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
