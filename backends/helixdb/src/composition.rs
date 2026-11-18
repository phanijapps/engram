//! Recipe-owned integration wrappers over retained typed native contributors.
use async_trait::async_trait;
use engram_domain::*;
use engram_integration::{KnowledgeQuery, storage_composition::*};
use engram_knowledge::{
    KnowledgeRepository, SourceBatchLedger, SourceBatchRecord, SourceBatchRecordKind,
};
use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryService};
use engram_runtime::{CoreError, CoreResult, PolicyAuthorizer, effect::ObservedResult};
use engram_store_helixdb::{
    HelixStore,
    composition_storage::{NativeCall, NativeSession},
};
use std::sync::Arc;

#[derive(Clone)]
pub struct KnowledgePorts {
    pub store: HelixStore,
    call: Option<Arc<NativeCall>>,
}
impl KnowledgePorts {
    pub fn new(store: HelixStore) -> Self {
        Self { store, call: None }
    }
    fn validate_live(&self) -> CoreResult<()> {
        if let Some(call) = &self.call {
            call.validate_live()?;
        }
        Ok(())
    }
    fn complete<T>(&self, result: CoreResult<T>) -> CoreResult<T> {
        match &self.call {
            Some(call) => call.complete(result),
            None => result,
        }
    }
    fn scope(&self, scope: &Scope) -> CoreResult<()> {
        if scope == self.store.composition_scope() {
            Ok(())
        } else {
            Err(CoreError::Conflict {
                reason: "recipe contributor scope mismatch".into(),
            })
        }
    }
}
pub struct HelixComposition {
    store: HelixStore,
    authorizer: Arc<dyn PolicyAuthorizer>,
}
impl HelixComposition {
    pub fn new(store: HelixStore, authorizer: Arc<dyn PolicyAuthorizer>) -> Self {
        Self { store, authorizer }
    }
}
struct RecipeSession {
    native: NativeSession,
    authorizer: Arc<dyn PolicyAuthorizer>,
}
impl StorageComposition for HelixComposition {
    fn session(&self, scope: &Scope) -> CoreResult<Arc<dyn CompositionSession>> {
        Ok(Arc::new(RecipeSession {
            native: self.store.native_session(scope)?,
            authorizer: self.authorizer.clone(),
        }))
    }
}
struct Observation(Arc<NativeCall>);
impl CallObservation for Observation {
    fn finish(&self, failed: bool) -> CoreResult<engram_runtime::effect::EffectObservation> {
        self.0.observation(failed)
    }
}
impl CompositionSession for RecipeSession {
    fn binding(&self) -> SessionBinding {
        let b = self.native.binding();
        SessionBinding {
            backend: b.backend.clone(),
            mode: b.mode.clone(),
            store: b.store.clone(),
            session: b.session.clone(),
            scope: b.scope.clone(),
        }
    }
    fn scope(&self) -> &Scope {
        &self.native.binding().scope
    }
    fn begin_call(&self, actor: &str, item: &str) -> ObservedResult<CompositionCall> {
        let opened = self.native.begin_call(actor, item);
        ObservedResult {
            result: opened.result.map(|native| {
                let native = Arc::new(native);
                let store = native.store();
                CompositionCall {
                    identity: native.identity().clone(),
                    ports: CallPorts {
                        memory: Arc::new(CallMemory {
                            inner: Arc::new(store.memory_service(self.authorizer.clone())),
                            call: native.clone(),
                        }),
                        knowledge: Arc::new(KnowledgePorts {
                            store,
                            call: Some(native.clone()),
                        }),
                    },
                    observation: Arc::new(Observation(native)),
                }
            }),
            observation: opened.observation,
        }
    }
}
struct CallMemory {
    inner: Arc<dyn MemoryService>,
    call: Arc<NativeCall>,
}

#[async_trait]
impl MemoryRepository for CallMemory {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        self.call.validate_live()?;
        self.call.complete(self.inner.put_memory(record).await)
    }
    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        self.call.validate_live()?;
        self.call.complete(self.inner.get_memory(id, scope).await)
    }
    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent> {
        self.call.validate_live()?;
        self.call.complete(self.inner.append_event(event).await)
    }
    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        self.call.validate_live()?;
        self.call
            .complete(self.inner.update_memory_status(id, scope, status).await)
    }
    async fn list_memories_paged(
        &self,
        scope: &Scope,
        after: Option<&Cursor>,
        limit: usize,
    ) -> CoreResult<Page<MemoryRecord>> {
        self.call.validate_live()?;
        self.call
            .complete(self.inner.list_memories_paged(scope, after, limit).await)
    }
}

#[async_trait]
impl MemoryEventRepository for CallMemory {
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        self.call.validate_live()?;
        self.call.complete(self.inner.get_event(id, scope).await)
    }
    async fn list_events_for_memory(
        &self,
        id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        self.call.validate_live()?;
        self.call
            .complete(self.inner.list_events_for_memory(id, scope).await)
    }
    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        self.call.validate_live()?;
        self.call
            .complete(self.inner.list_events_for_scope(scope).await)
    }
}

#[async_trait]
impl MemoryService for CallMemory {
    async fn write_memory(&self, request: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse> {
        self.call.validate_live()?;
        self.call.complete(self.inner.write_memory(request).await)
    }
    async fn retrieve(&self, request: RetrievalRequest) -> CoreResult<ContextPayload> {
        self.call.validate_live()?;
        self.call.complete(self.inner.retrieve(request).await)
    }
    async fn forget(&self, request: ForgetRequest) -> CoreResult<ForgetResult> {
        self.call.validate_live()?;
        self.call.complete(self.inner.forget(request).await)
    }
}

#[async_trait]
impl KnowledgeRepository for KnowledgePorts {
    async fn put_source(&self, record: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        self.validate_live()?;
        self.complete(self.store.put_source(record).await)
    }
    async fn put_document(&self, record: SourceDocument) -> CoreResult<SourceDocument> {
        self.validate_live()?;
        self.complete(self.store.put_document(record).await)
    }
    async fn put_chunk(&self, record: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        self.validate_live()?;
        self.complete(self.store.put_chunk(record).await)
    }
    async fn put_entity(&self, record: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        self.validate_live()?;
        self.complete(self.store.put_entity(record).await)
    }
    async fn put_relationship(
        &self,
        record: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        self.validate_live()?;
        self.complete(self.store.put_relationship(record).await)
    }
    async fn get_source(
        &self,
        id: &SourceId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeSource>> {
        self.validate_live()?;
        self.complete(self.store.get_source(id, scope).await)
    }
    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>> {
        self.validate_live()?;
        self.complete(self.store.get_chunk(id, scope).await)
    }
    async fn get_entity(
        &self,
        id: &EntityId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        self.validate_live()?;
        self.complete(self.store.get_entity(id, scope).await)
    }
    async fn get_relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        self.validate_live()?;
        self.complete(self.store.get_relationship(id, scope).await)
    }
    async fn list_sources(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeSource>> {
        self.validate_live()?;
        self.complete(self.store.list_sources(scope).await)
    }
    async fn list_documents(&self, scope: &Scope) -> CoreResult<Vec<SourceDocument>> {
        self.validate_live()?;
        self.complete(self.store.list_documents(scope).await)
    }
    async fn list_entities_by_source_ref(
        &self,
        source: &str,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        self.validate_live()?;
        self.complete(self.store.list_entities_by_source_ref(source, scope).await)
    }
    async fn list_relationships_by_source_ref(
        &self,
        source: &str,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.validate_live()?;
        self.complete(
            self.store
                .list_relationships_by_source_ref(source, scope)
                .await,
        )
    }
    async fn restore_entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<bool> {
        self.validate_live()?;
        self.complete(self.store.restore_entity(id, scope).await)
    }
    async fn restore_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool> {
        self.validate_live()?;
        self.complete(self.store.restore_relationship(id, scope).await)
    }
}
#[async_trait]
impl CompositionKnowledge for KnowledgePorts {
    async fn put_source_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeSource,
    ) -> CoreResult<KnowledgeSource> {
        self.scope(expected)?;
        self.put_source(record).await
    }
    async fn put_document_in_scope(
        &self,
        expected: &Scope,
        record: SourceDocument,
    ) -> CoreResult<SourceDocument> {
        self.scope(expected)?;
        self.put_document(record).await
    }
    async fn put_chunk_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeChunk,
    ) -> CoreResult<KnowledgeChunk> {
        self.scope(expected)?;
        self.put_chunk(record).await
    }
    async fn put_entity_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeEntity,
    ) -> CoreResult<KnowledgeEntity> {
        self.scope(expected)?;
        self.put_entity(record).await
    }
    async fn put_relationship_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        self.scope(expected)?;
        self.put_relationship(record).await
    }
    async fn registry_source(&self, id: &SourceId) -> CoreResult<Option<KnowledgeSource>> {
        self.complete(
            self.store
                .get_source(id, self.store.composition_scope())
                .await,
        )
    }
}
#[async_trait]
impl SourceBatchLedger for KnowledgePorts {
    async fn list_source_batch_records(
        &self,
        scope: &Scope,
        source_id: &str,
    ) -> CoreResult<Vec<SourceBatchRecord>> {
        self.validate_live()?;
        self.complete(self.store.list_source_batch_records(scope, source_id).await)
    }
    async fn record_source_batch_writes(
        &self,
        scope: &Scope,
        source_id: &str,
        batch_key: &str,
        kind: SourceBatchRecordKind,
        record_ids: &[String],
    ) -> CoreResult<()> {
        self.validate_live()?;
        self.complete(
            self.store
                .record_source_batch_writes(scope, source_id, batch_key, kind, record_ids)
                .await,
        )
    }
    async fn mark_source_batch_superseded(
        &self,
        scope: &Scope,
        source_id: &str,
        batch_key: &str,
        kind: SourceBatchRecordKind,
        record_id: &str,
    ) -> CoreResult<()> {
        self.validate_live()?;
        self.complete(
            self.store
                .mark_source_batch_superseded(scope, source_id, batch_key, kind, record_id)
                .await,
        )
    }
}
#[async_trait]
impl KnowledgeQuery for KnowledgePorts {
    async fn list_entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        self.validate_live()?;
        self.complete(self.store.composition_inventory(scope).map(|s| s.entities))
    }
    async fn list_relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.validate_live()?;
        self.complete(
            self.store
                .composition_inventory(scope)
                .map(|s| s.relationships),
        )
    }
    async fn list_chunks(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeChunk>> {
        self.validate_live()?;
        self.complete(self.store.composition_inventory(scope).map(|s| s.chunks))
    }
    async fn list_graphs(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeGraph>> {
        self.validate_live()?;
        self.complete(self.store.composition_inventory(scope).map(|s| s.graphs))
    }
    async fn list_chunks_by_document(
        &self,
        id: &DocumentId,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeChunk>> {
        self.validate_live()?;
        self.complete(self.store.list_chunks_by_document(id, scope).await)
    }
}
#[async_trait]
impl ArchiveInventory for KnowledgePorts {
    async fn snapshot(&self, scope: &Scope) -> CoreResult<ArchiveSnapshot> {
        let s = self.store.composition_archive_inventory(scope)?;
        Ok(ArchiveSnapshot {
            memories: s.memories,
            sources: s.sources,
            documents: s.documents,
            chunks: s.chunks,
            entities: s.entities,
            relationships: s.relationships,
            concept_schemes: s.concept_schemes,
            concepts: s.concepts,
            beliefs: s.beliefs,
            hierarchy_nodes: s.hierarchy_nodes,
        })
    }
}
