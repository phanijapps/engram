//! Canonical governed memory orchestration over portable storage.
use crate::{
    Clock, CoreResult, IdGenerator, MemoryEventRepository, MemoryRepository, MemoryService,
    MemoryStorage, PolicyAuthorizer,
};
use async_trait::async_trait;
use engram_domain::*;
use std::sync::Arc;
#[derive(Clone)]
pub struct GovernedMemoryService {
    pub(crate) store: Arc<dyn MemoryStorage>,
    pub(crate) authorizer: Arc<dyn PolicyAuthorizer>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) ids: Arc<dyn IdGenerator>,
}
impl GovernedMemoryService {
    pub fn new(
        store: Arc<dyn MemoryStorage>,
        authorizer: Arc<dyn PolicyAuthorizer>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            store,
            authorizer,
            clock,
            ids,
        }
    }
}
#[async_trait]
impl MemoryService for GovernedMemoryService {
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
impl MemoryRepository for GovernedMemoryService {
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
    async fn list_memories_paged(
        &self,
        scope: &Scope,
        after: Option<&Cursor>,
        limit: usize,
    ) -> CoreResult<Page<MemoryRecord>> {
        self.store.list_memories_paged(scope, after, limit).await
    }
}
#[async_trait]
impl MemoryEventRepository for GovernedMemoryService {
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
