//! SQL-backed memory service orchestration.
//!
//! This module composes the SQLite repository with policy, clock, and ID
//! dependencies. Operation-specific behavior lives in `write`, `retrieval`, and
//! `forget` to avoid a monolithic SQL service.

use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use engram_domain::*;
use engram_memory::{
    Clock, CoreResult, IdGenerator, MemoryEventRepository, MemoryRepository, MemoryService,
    PolicyAuthorizer,
};

use crate::{
    dependencies::{AllowAllPolicyAuthorizer, SequentialIdGenerator, SystemClock},
    service::SqlMemoryStore,
};

/// SQL-backed implementation of the Engram memory service.
///
/// The service composes a `SqlMemoryStore` with policy, clock, and ID ports.
/// Operation orchestration stays outside the repository layer so storage can
/// evolve without changing service-level contract behavior.
#[derive(Clone)]
pub struct SqlMemoryService {
    pub(crate) store: SqlMemoryStore,
    pub(crate) authorizer: Arc<dyn PolicyAuthorizer>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) ids: Arc<dyn IdGenerator>,
}

impl SqlMemoryService {
    /// Opens an in-memory SQLite service with default local dependencies.
    ///
    /// This constructor is intended for conformance tests and examples. It
    /// proves SQL semantics without requiring an external database process.
    pub fn open_in_memory() -> CoreResult<Self> {
        Self::with_dependencies(
            SqlMemoryStore::open_in_memory()?,
            Arc::new(AllowAllPolicyAuthorizer),
            Arc::new(SystemClock),
            Arc::new(SequentialIdGenerator::new()),
        )
    }

    /// Opens a file-backed SQLite service with default local dependencies.
    ///
    /// This constructor is intended for durable local smoke tests and embedded
    /// development. It uses the same orchestration, policy, clock, and ID
    /// defaults as `open_in_memory`. The local ID generator is seeded past the
    /// highest existing ID on disk so reopening an existing database never
    /// collides with rows a previous process wrote.
    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        let store = SqlMemoryStore::open_file(path)?;
        let ids = SequentialIdGenerator::new();
        if let Ok(used) = store.max_used_id_suffix() {
            ids.advance_past(used);
        }
        Self::with_dependencies(
            store,
            Arc::new(AllowAllPolicyAuthorizer),
            Arc::new(SystemClock),
            Arc::new(ids),
        )
    }

    /// Creates a SQL service with explicit storage and behavior dependencies.
    ///
    /// Use this constructor when tests need fixed clocks, scripted IDs, or
    /// stricter authorization while preserving the same SQLite repository.
    pub fn with_dependencies(
        store: SqlMemoryStore,
        authorizer: Arc<dyn PolicyAuthorizer>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> CoreResult<Self> {
        Ok(Self {
            store,
            authorizer,
            clock,
            ids,
        })
    }

    /// Lists memory records visible to `scope` (store-specific; not on a port).
    ///
    /// Export and maintenance paths need a scope-wide enumeration of stored
    /// memories. The store's internal `list_memories` returns every record;
    /// this wrapper applies the same visibility rule retrieval uses (tenant must
    /// match, optional scope dimensions narrow) so callers never see records
    /// outside their scope.
    pub fn list_memories_in_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryRecord>> {
        let mut records = self.store.list_memories()?;
        records.retain(|record| crate::scope::scope_allows(&record.scope, scope));
        Ok(records)
    }
}

#[async_trait]
impl MemoryService for SqlMemoryService {
    async fn write_memory(&self, request: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse> {
        crate::write::write_memory(self, request).await
    }

    async fn retrieve(&self, request: RetrievalRequest) -> CoreResult<ContextPayload> {
        crate::retrieval::retrieve(self, request).await
    }

    async fn forget(&self, request: ForgetRequest) -> CoreResult<ForgetResult> {
        crate::forget::forget(self, request).await
    }
}

#[async_trait]
impl MemoryRepository for SqlMemoryService {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        self.store.put_memory(record).await
    }

    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        self.store.get_memory(id, scope).await
    }

    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent> {
        self.store.append_event(event).await
    }

    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        self.store.update_memory_status(id, scope, status).await
    }
}

#[async_trait]
impl MemoryEventRepository for SqlMemoryService {
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        self.store.get_event(id, scope).await
    }

    async fn list_events_for_memory(
        &self,
        memory_id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        self.store.list_events_for_memory(memory_id, scope).await
    }

    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        self.store.list_events_for_scope(scope).await
    }
}
