//! Service and repository implementation for the in-memory adapter.
//!
//! This module wires core traits to process-local state. Operation-specific
//! behavior is delegated to `write`, `retrieval`, and `forget` so construction,
//! repository access, and request orchestration do not collapse into one large
//! module.

use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use engram_core::{
    Clock, CoreError, CoreResult, IdGenerator, MemoryEventRepository, MemoryRepository,
    MemoryService, PolicyAuthorizer, RetrievalFusion, RetrievalIndex,
};
use engram_domain::*;
use engram_retrieval::WeightedRetrievalFusion;

use crate::{
    dependencies::{AllowAllPolicyAuthorizer, SequentialIdGenerator, SystemClock},
    scope::scope_allows,
    state::InMemoryState,
};

/// In-memory implementation for early Engram memory slices.
///
/// This service supports writes, exact/keyword retrieval, record lookup,
/// lifecycle event lookup, forget lifecycle behavior, and status updates
/// against process-local state.
#[derive(Clone)]
pub struct InMemoryMemoryService {
    pub(crate) state: Arc<Mutex<InMemoryState>>,
    pub(crate) authorizer: Arc<dyn PolicyAuthorizer>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) ids: Arc<dyn IdGenerator>,
    pub(crate) retrieval_fusion: Arc<dyn RetrievalFusion>,
    pub(crate) retrieval_indexes: Vec<Arc<dyn RetrievalIndex>>,
}

impl InMemoryMemoryService {
    /// Creates an in-memory service with permissive policy and default local
    /// clock/ID dependencies.
    ///
    /// This constructor is meant for spec tests and examples. It still routes
    /// writes and retrieval through core ports so stricter dependencies can be
    /// injected without changing operation behavior.
    pub fn new() -> Self {
        Self::with_dependencies(
            Arc::new(AllowAllPolicyAuthorizer),
            Arc::new(SystemClock),
            Arc::new(SequentialIdGenerator::new()),
        )
    }

    /// Creates an in-memory service with an injected policy authorizer.
    ///
    /// Tests can use this to prove denied writes or retrievals do not create
    /// records, events, or leaked context while retaining default local time and
    /// ID generation.
    pub fn with_authorizer(authorizer: Arc<dyn PolicyAuthorizer>) -> Self {
        Self::with_dependencies(
            authorizer,
            Arc::new(SystemClock),
            Arc::new(SequentialIdGenerator::new()),
        )
    }

    /// Creates an in-memory service with all behavior dependencies injected.
    ///
    /// This constructor is the deterministic test boundary for exact
    /// timestamps, stable identifiers, and policy outcomes. Production code
    /// should use a durable adapter rather than relying on this process-local
    /// store.
    pub fn with_dependencies(
        authorizer: Arc<dyn PolicyAuthorizer>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self::with_retrieval_fusion(
            authorizer,
            clock,
            ids,
            Arc::new(WeightedRetrievalFusion::default()),
        )
    }

    /// Creates an in-memory service with an injected retrieval fusion strategy.
    ///
    /// This constructor is the adapter composition boundary for retrieval
    /// experiments. Candidate production remains in this crate, while fusion can
    /// be swapped without changing core traits or public JSON contracts.
    pub fn with_retrieval_fusion(
        authorizer: Arc<dyn PolicyAuthorizer>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        retrieval_fusion: Arc<dyn RetrievalFusion>,
    ) -> Self {
        Self::with_retrieval_fusion_and_indexes(
            authorizer,
            clock,
            ids,
            retrieval_fusion,
            Vec::new(),
        )
    }

    /// Creates an in-memory service with injected external retrieval indexes.
    ///
    /// This constructor composes semantic, graph, or other candidate sources
    /// behind `RetrievalIndex` without making the in-memory adapter depend on a
    /// concrete vector store or provider. The default weighted fusion strategy
    /// still owns cross-source ranking.
    pub fn with_retrieval_indexes(
        authorizer: Arc<dyn PolicyAuthorizer>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        retrieval_indexes: Vec<Arc<dyn RetrievalIndex>>,
    ) -> Self {
        Self::with_retrieval_fusion_and_indexes(
            authorizer,
            clock,
            ids,
            Arc::new(WeightedRetrievalFusion::default()),
            retrieval_indexes,
        )
    }

    /// Creates an in-memory service with injected fusion and retrieval indexes.
    ///
    /// Tests and integration layers can use this as the full retrieval
    /// composition boundary while keeping concrete adapter dependencies outside
    /// this crate.
    pub fn with_retrieval_fusion_and_indexes(
        authorizer: Arc<dyn PolicyAuthorizer>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        retrieval_fusion: Arc<dyn RetrievalFusion>,
        retrieval_indexes: Vec<Arc<dyn RetrievalIndex>>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(InMemoryState::default())),
            authorizer,
            clock,
            ids,
            retrieval_fusion,
            retrieval_indexes,
        }
    }

    /// Locks process-local state and translates lock poisoning into `CoreError`.
    ///
    /// Operation modules use this helper so adapter-specific synchronization
    /// failures stay behind the stable core error surface.
    pub(crate) fn lock_state(&self) -> CoreResult<MutexGuard<'_, InMemoryState>> {
        self.state.lock().map_err(|_| CoreError::Adapter {
            adapter: "engram-store-memory".to_owned(),
            message: "state lock poisoned".to_owned(),
        })
    }
}

impl Default for InMemoryMemoryService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryRepository for InMemoryMemoryService {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        let mut state = self.lock_state()?;
        state.memories.insert(record.id.to_string(), record.clone());
        Ok(record)
    }

    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        let state = self.lock_state()?;
        let memory = state
            .memories
            .get(id.as_str())
            .filter(|record| scope_allows(&record.scope, scope));
        Ok(memory.cloned())
    }

    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent> {
        let mut state = self.lock_state()?;
        state.events.push(event.clone());
        Ok(event)
    }

    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        let mut state = self.lock_state()?;
        let record = state
            .memories
            .get_mut(id.as_str())
            .filter(|record| scope_allows(&record.scope, scope))
            .ok_or_else(|| CoreError::NotFound {
                target_type: "memory",
                target_id: id.to_string(),
            })?;
        record.status = status;
        record.updated_at = Some(self.clock.now());
        Ok(record.clone())
    }
}

#[async_trait]
impl MemoryEventRepository for InMemoryMemoryService {
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        let state = self.lock_state()?;
        let event = state
            .events
            .iter()
            .find(|event| event.id == *id && scope_allows(&event.scope, scope));
        Ok(event.cloned())
    }

    async fn list_events_for_memory(
        &self,
        memory_id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        let state = self.lock_state()?;
        Ok(state
            .events
            .iter()
            .filter(|event| {
                event.memory_id.as_ref() == Some(memory_id) && scope_allows(&event.scope, scope)
            })
            .cloned()
            .collect())
    }

    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        let state = self.lock_state()?;
        Ok(state
            .events
            .iter()
            .filter(|event| scope_allows(&event.scope, scope))
            .cloned()
            .collect())
    }
}

#[async_trait]
impl MemoryService for InMemoryMemoryService {
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
