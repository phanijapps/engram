//! Typed storage contributions to shared integration algorithms.
use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::CoreResult;

/// Existing v1 export families, read in the caller's scope. No wire representation lives here.
#[derive(Default)]
pub struct ArchiveSnapshot {
    pub memories: Vec<MemoryRecord>,
    pub sources: Vec<KnowledgeSource>,
    pub documents: Vec<SourceDocument>,
    pub chunks: Vec<KnowledgeChunk>,
    pub entities: Vec<KnowledgeEntity>,
    pub relationships: Vec<KnowledgeRelationship>,
    pub concept_schemes: Vec<ConceptScheme>,
    /// Concepts inherit the owning scheme's complete scope.
    pub concepts: Vec<(Concept, Scope)>,
    pub beliefs: Vec<Belief>,
    pub hierarchy_nodes: Vec<HierarchyNode>,
}
#[async_trait]
pub trait ArchiveInventory: Send + Sync {
    async fn snapshot(&self, scope: &Scope) -> CoreResult<ArchiveSnapshot>;
}

use engram_knowledge::{
    KnowledgeRepository, SourceBatchLedger, SourceBatchRecord, SourceBatchRecordKind,
};
use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryService};
use engram_runtime::{
    CoreError,
    effect::{CleanupCertainty, EffectIdentity, EffectObservation, EffectOutcome, ObservedResult},
};
use std::{
    collections::HashSet,
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[async_trait]
pub trait CompositionRead: KnowledgeRepository + crate::KnowledgeQuery {}
impl<T: KnowledgeRepository + crate::KnowledgeQuery + ?Sized> CompositionRead for T {}

#[async_trait]
pub trait CompositionKnowledge: CompositionRead + SourceBatchLedger {
    /// Identity collision check; this read never grants authority to the returned scope.
    async fn registry_source(&self, id: &SourceId) -> CoreResult<Option<KnowledgeSource>>;
    /// Validate expected full scope, parents and existing ownership atomically with the write.
    async fn put_source_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeSource,
    ) -> CoreResult<KnowledgeSource>;

    /// Validate expected full scope, parents and existing ownership atomically with the write.
    async fn put_document_in_scope(
        &self,
        expected: &Scope,
        record: SourceDocument,
    ) -> CoreResult<SourceDocument>;

    /// Validate expected full scope, parents and existing ownership atomically with the write.
    async fn put_chunk_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeChunk,
    ) -> CoreResult<KnowledgeChunk>;

    /// Validate expected full scope, parents and existing ownership atomically with the write.
    async fn put_entity_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeEntity,
    ) -> CoreResult<KnowledgeEntity>;

    /// Validate expected full scope, parents and existing ownership atomically with the write.
    async fn put_relationship_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship>;
}
#[derive(Clone)]
pub struct CallPorts {
    pub memory: Arc<dyn MemoryService>,
    pub knowledge: Arc<dyn CompositionKnowledge>,
}
pub trait CallObservation: Send + Sync {
    fn finish(&self, failed: bool) -> CoreResult<EffectObservation>;
}
pub struct CompositionCall {
    pub ports: CallPorts,
    pub identity: EffectIdentity,
    pub observation: Arc<dyn CallObservation>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionBinding {
    pub backend: String,
    pub mode: String,
    pub store: String,
    pub session: String,
    pub scope: Scope,
}
impl SessionBinding {
    fn matches(&self, id: &EffectIdentity, actor: &str, item: &str) -> bool {
        id.backend == self.backend
            && id.mode == self.mode
            && id.store == self.store
            && id.session == self.session
            && id.scope == self.scope
            && id.actor == actor
            && id.item == item
            && !id.call.is_empty()
    }
}
pub trait CompositionSession: Send + Sync {
    fn binding(&self) -> SessionBinding;
    fn scope(&self) -> &Scope;
    fn begin_call(&self, actor: &str, item: &str) -> ObservedResult<CompositionCall>;
}
pub trait StorageComposition: Send + Sync {
    fn session(&self, scope: &Scope) -> CoreResult<Arc<dyn CompositionSession>>;
}

/// Definite local-port sessions; native recipes supply their own session-bound contributors.
pub struct PortStorageComposition {
    ports: CallPorts,
    backend: String,
    mode: String,
    store: String,
    reservations: Arc<Mutex<HashSet<String>>>,
}
impl PortStorageComposition {
    pub fn new(
        ports: CallPorts,
        backend: impl Into<String>,
        mode: impl Into<String>,
        store: impl Into<String>,
    ) -> Self {
        Self {
            ports,
            backend: backend.into(),
            mode: mode.into(),
            store: store.into(),
            reservations: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}
struct PortSession {
    ports: CallPorts,
    scope: Scope,
    backend: String,
    mode: String,
    store: String,
    id: String,
    reservations: Arc<Mutex<HashSet<String>>>,
    key: String,
}
impl Drop for PortSession {
    fn drop(&mut self) {
        if let Ok(mut keys) = self.reservations.lock() {
            keys.remove(&self.key);
        }
    }
}
static TOKENS: AtomicU64 = AtomicU64::new(1);
impl StorageComposition for PortStorageComposition {
    fn session(&self, scope: &Scope) -> CoreResult<Arc<dyn CompositionSession>> {
        let key = serde_json::to_string(scope).map_err(|_| CoreError::InvalidRequest {
            reason: "invalid composition scope".into(),
        })?;
        if !self
            .reservations
            .lock()
            .map_err(|_| CoreError::Conflict {
                reason: "composition reservation unavailable".into(),
            })?
            .insert(key.clone())
        {
            return Err(CoreError::Conflict {
                reason: "composition scope already reserved".into(),
            });
        }
        Ok(Arc::new(PortSession {
            ports: self.ports.clone(),
            scope: scope.clone(),
            backend: self.backend.clone(),
            mode: self.mode.clone(),
            store: self.store.clone(),
            id: format!("session-{}", TOKENS.fetch_add(1, Ordering::Relaxed)),
            reservations: self.reservations.clone(),
            key,
        }))
    }
}
struct CertainCall(EffectIdentity);
impl CallObservation for CertainCall {
    fn finish(&self, failed: bool) -> CoreResult<EffectObservation> {
        Ok(EffectObservation {
            identity: self.0.clone(),
            step: None,
            commit_id: None,
            outcome: if failed {
                EffectOutcome::DefiniteFailure
            } else {
                EffectOutcome::Acknowledged
            },
            cleanup: CleanupCertainty::Confirmed,
        })
    }
}
impl CompositionSession for PortSession {
    fn binding(&self) -> SessionBinding {
        SessionBinding {
            backend: self.backend.clone(),
            mode: self.mode.clone(),
            store: self.store.clone(),
            session: self.id.clone(),
            scope: self.scope.clone(),
        }
    }

    fn scope(&self) -> &Scope {
        &self.scope
    }
    fn begin_call(&self, actor: &str, item: &str) -> ObservedResult<CompositionCall> {
        let identity = EffectIdentity {
            actor: actor.into(),
            item: item.into(),
            call: format!("call-{}", TOKENS.fetch_add(1, Ordering::Relaxed)),
            session: self.id.clone(),
            backend: self.backend.clone(),
            mode: self.mode.clone(),
            store: self.store.clone(),
            scope: self.scope.clone(),
        };
        ObservedResult {
            result: Ok(CompositionCall {
                ports: self.ports.clone(),
                identity: identity.clone(),
                observation: Arc::new(CertainCall(identity.clone())),
            }),
            observation: EffectObservation {
                identity,
                step: None,
                commit_id: None,
                outcome: EffectOutcome::Staged,
                cleanup: CleanupCertainty::Confirmed,
            },
        }
    }
}
#[derive(Debug, Default)]
pub struct OperationState {
    pub stopped: Option<EffectObservation>,
    /// Earlier definite diagnostics stay local when uncertainty takes precedence.
    pub diagnostics: Vec<CoreError>,
}
impl OperationState {
    pub fn stopped(&self) -> bool {
        self.stopped.is_some()
    }
}
#[derive(Clone)]
pub struct ObservedPorts {
    session: Arc<dyn CompositionSession>,
    pub state: Arc<Mutex<OperationState>>,
}
fn canonical_operation_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                *c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(c)
            }
        })
        && b[14] == b'4'
        && [b'8', b'9', b'a', b'b'].contains(&b[19])
}
fn valid_observation(o: &EffectObservation) -> bool {
    if o.step.as_deref().is_some_and(str::is_empty)
        || o.commit_id
            .as_deref()
            .is_some_and(|id| !canonical_operation_id(id) || o.step.is_none())
    {
        return false;
    }
    match &o.outcome {
        EffectOutcome::Unknown { operation_id } => {
            canonical_operation_id(operation_id)
                && o.step.is_some()
                && o.commit_id.as_ref() == Some(operation_id)
        }
        EffectOutcome::BlockedByPriorUnknown { operation_id } => {
            canonical_operation_id(operation_id) && o.step.is_none() && o.commit_id.is_none()
        }
        EffectOutcome::Cancelled => o.step.is_some() && o.commit_id.is_none(),
        _ => true,
    }
}
fn invalid_observation(mut o: EffectObservation) -> EffectObservation {
    o.outcome = EffectOutcome::DefiniteFailure;
    o.cleanup = CleanupCertainty::Unconfirmed;
    o.step = None;
    o.commit_id = None;
    o
}
fn stop_error(observation: &EffectObservation) -> CoreError {
    let message = match &observation.outcome {
        _ if !valid_observation(observation) => {
            "composition effect evidence is invalid; stop logical work".into()
        }
        EffectOutcome::Unknown { operation_id } => {
            format!("operation outcome unknown: {operation_id}")
        }
        EffectOutcome::BlockedByPriorUnknown { operation_id } => {
            format!("blocked by prior uncertain operation: {operation_id}")
        }
        _ => "composition contribution did not complete; stop logical work".into(),
    };
    CoreError::Adapter {
        adapter: "composition".into(),
        message,
    }
}
impl ObservedPorts {
    pub fn new(session: Arc<dyn CompositionSession>) -> Self {
        Self {
            session,
            state: Arc::new(Mutex::new(OperationState::default())),
        }
    }
    fn require_scope(&self, actual: &Scope) -> CoreResult<()> {
        if actual == self.session.scope() {
            Ok(())
        } else {
            Err(CoreError::Conflict {
                reason: "composition request scope differs from session".into(),
            })
        }
    }
    pub fn stopped(&self) -> bool {
        self.state.lock().map_or(true, |s| s.stopped())
    }
    pub fn stopped_error(&self) -> Option<CoreError> {
        match self.state.lock() {
            Ok(s) => s.stopped.as_ref().map(stop_error),
            Err(_) => Some(CoreError::Conflict {
                reason: "composition effect state unavailable".into(),
            }),
        }
    }
    pub fn supersede_failure(&self, prior: Option<CoreError>) -> Option<CoreError> {
        if let Some(error) = self.stopped_error() {
            if let Some(prior) = prior {
                if let Ok(mut state) = self.state.lock() {
                    state.diagnostics.push(prior);
                }
            }
            Some(error)
        } else {
            prior
        }
    }
    async fn invoke<T, F, Fut>(&self, actor: &str, item: &str, work: F) -> CoreResult<T>
    where
        F: FnOnce(CallPorts) -> Fut,
        Fut: Future<Output = CoreResult<T>>,
    {
        if let Some(error) = self.stopped_error() {
            return Err(error);
        }
        let opened = self.session.begin_call(actor, item);
        let binding = self.session.binding();
        if binding.scope != *self.session.scope()
            || !binding.matches(&opened.observation.identity, actor, item)
            || !valid_observation(&opened.observation)
            || (opened.result.is_ok() && opened.observation.outcome != EffectOutcome::Staged)
            || opened
                .result
                .as_ref()
                .is_ok_and(|call| call.identity != opened.observation.identity)
        {
            let observation = invalid_observation(opened.observation);
            self.state
                .lock()
                .map_err(|_| CoreError::Conflict {
                    reason: "composition state unavailable".into(),
                })?
                .stopped = Some(observation);
            return Err(CoreError::Conflict {
                reason: "composition session/call binding mismatch".into(),
            });
        }
        let (result, observation) =
            match opened.result {
                Err(error) => (Err(error), opened.observation),
                Ok(call) => {
                    let result = work(call.ports.clone()).await;
                    let observation = call.observation.finish(result.is_err());
                    match observation {
                        Ok(observation)
                            if observation.identity == call.identity
                                && observation.identity.scope == *self.session.scope()
                                && valid_observation(&observation)
                                && opened
                                    .observation
                                    .step
                                    .as_ref()
                                    .is_none_or(|step| observation.step.as_ref() == Some(step))
                                && opened.observation.commit_id.as_ref().is_none_or(|id| {
                                    observation.commit_id.as_ref() == Some(id)
                                }) =>
                        {
                            (result, observation)
                        }
                        _ => {
                            let observation = invalid_observation(opened.observation);
                            (
                                Err(CoreError::Conflict {
                                    reason: "composition call observation mismatch".into(),
                                }),
                                observation,
                            )
                        }
                    }
                }
            };
        let halt = observation.cleanup == CleanupCertainty::Unconfirmed
            || matches!(
                observation.outcome,
                EffectOutcome::Unknown { .. }
                    | EffectOutcome::BlockedByPriorUnknown { .. }
                    | EffectOutcome::Staged
                    | EffectOutcome::Cancelled
            );
        if halt {
            let error = stop_error(&observation);
            if let Ok(mut state) = self.state.lock() {
                if let Err(diagnostic) = result {
                    state.diagnostics.push(diagnostic);
                }
                state.stopped = Some(observation);
            }
            return Err(error);
        }
        result
    }
}

#[async_trait]
impl MemoryRepository for ObservedPorts {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        self.require_scope(&record.scope)?;
        let actor: String = record.provenance.actor.id.to_string();
        let item: String = record.id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.memory.put_memory(record).await
        })
        .await
    }
    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.memory.get_memory(id, scope).await
        })
        .await
    }
    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent> {
        self.require_scope(&event.scope)?;
        let actor: String = event.actor.id.to_string();
        let item: String = event.id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.memory.append_event(event).await
        })
        .await
    }
    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.memory.update_memory_status(id, scope, status).await
        })
        .await
    }
}

#[async_trait]
impl MemoryEventRepository for ObservedPorts {
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.memory.get_event(id, scope).await
        })
        .await
    }
    async fn list_events_for_memory(
        &self,
        memory_id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = memory_id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.memory.list_events_for_memory(memory_id, scope).await
        })
        .await
    }
    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = "memory-events".into();
        self.invoke(&actor, &item, |ports| async move {
            ports.memory.list_events_for_scope(scope).await
        })
        .await
    }
}

#[async_trait]
impl MemoryService for ObservedPorts {
    async fn write_memory(&self, request: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse> {
        self.require_scope(&request.scope)?;
        let actor: String = request.requester.actor.id.to_string();
        let item: String = request
            .idempotency_key
            .clone()
            .unwrap_or_else(|| "memory-write".into());
        self.invoke(&actor, &item, |ports| async move {
            ports.memory.write_memory(request).await
        })
        .await
    }
    async fn retrieve(&self, request: RetrievalRequest) -> CoreResult<ContextPayload> {
        self.require_scope(&request.scope)?;
        let actor: String = request.requester.actor.id.to_string();
        let item: String = "retrieve".into();
        self.invoke(&actor, &item, |ports| async move {
            ports.memory.retrieve(request).await
        })
        .await
    }
    async fn forget(&self, request: ForgetRequest) -> CoreResult<ForgetResult> {
        self.require_scope(&request.scope)?;
        let actor: String = request.requester.actor.id.to_string();
        let item: String = request.target_id.clone();
        self.invoke(&actor, &item, |ports| async move {
            ports.memory.forget(request).await
        })
        .await
    }
}

#[async_trait]
impl KnowledgeRepository for ObservedPorts {
    async fn put_source(&self, record: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        self.require_scope(&record.scope)?;
        let actor: String = record.provenance.actor.id.to_string();
        let item: String = record.id.to_string();
        let expected = self.session.scope().clone();
        self.invoke(&actor, &item, |ports| async move {
            ports.knowledge.put_source_in_scope(&expected, record).await
        })
        .await
    }
    async fn put_document(&self, record: SourceDocument) -> CoreResult<SourceDocument> {
        let actor: String = record.provenance.actor.id.to_string();
        let item: String = record.id.to_string();
        let expected = self.session.scope().clone();
        self.invoke(&actor, &item, |ports| async move {
            ports
                .knowledge
                .put_document_in_scope(&expected, record)
                .await
        })
        .await
    }
    async fn put_chunk(&self, record: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        let actor: String = record.provenance.actor.id.to_string();
        let item: String = record.id.to_string();
        let expected = self.session.scope().clone();
        self.invoke(&actor, &item, |ports| async move {
            ports.knowledge.put_chunk_in_scope(&expected, record).await
        })
        .await
    }
    async fn put_entity(&self, record: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        self.require_scope(&record.scope)?;
        let actor: String = record.provenance.actor.id.to_string();
        let item: String = record.id.to_string();
        let expected = self.session.scope().clone();
        self.invoke(&actor, &item, |ports| async move {
            ports.knowledge.put_entity_in_scope(&expected, record).await
        })
        .await
    }
    async fn put_relationship(
        &self,
        record: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        self.require_scope(&record.scope)?;
        let actor: String = record.provenance.actor.id.to_string();
        let item: String = record.id.to_string();
        let expected = self.session.scope().clone();
        self.invoke(&actor, &item, |ports| async move {
            ports
                .knowledge
                .put_relationship_in_scope(&expected, record)
                .await
        })
        .await
    }
    async fn get_source(
        &self,
        id: &SourceId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeSource>> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.knowledge.get_source(id, scope).await
        })
        .await
    }
    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.knowledge.get_chunk(id, scope).await
        })
        .await
    }
    async fn get_entity(
        &self,
        id: &EntityId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.knowledge.get_entity(id, scope).await
        })
        .await
    }
    async fn get_relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.knowledge.get_relationship(id, scope).await
        })
        .await
    }
    async fn restore_entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<bool> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.knowledge.restore_entity(id, scope).await
        })
        .await
    }
    async fn restore_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.knowledge.restore_relationship(id, scope).await
        })
        .await
    }
    async fn list_entities_by_source_ref(
        &self,
        source_id: &str,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = source_id.to_owned();
        self.invoke(&actor, &item, |ports| async move {
            ports
                .knowledge
                .list_entities_by_source_ref(source_id, scope)
                .await
        })
        .await
    }
    async fn list_relationships_by_source_ref(
        &self,
        source_id: &str,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = source_id.to_owned();
        self.invoke(&actor, &item, |ports| async move {
            ports
                .knowledge
                .list_relationships_by_source_ref(source_id, scope)
                .await
        })
        .await
    }
}

#[async_trait]
impl SourceBatchLedger for ObservedPorts {
    async fn list_source_batch_records(
        &self,
        scope: &Scope,
        source_id: &str,
    ) -> CoreResult<Vec<SourceBatchRecord>> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = source_id.to_owned();
        self.invoke(&actor, &item, |ports| async move {
            ports
                .knowledge
                .list_source_batch_records(scope, source_id)
                .await
        })
        .await
    }
    async fn record_source_batch_writes(
        &self,
        scope: &Scope,
        source_id: &str,
        batch_key: &str,
        kind: SourceBatchRecordKind,
        record_ids: &[String],
    ) -> CoreResult<()> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = source_id.to_owned();
        self.invoke(&actor, &item, |ports| async move {
            ports
                .knowledge
                .record_source_batch_writes(scope, source_id, batch_key, kind, record_ids)
                .await
        })
        .await
    }
    async fn mark_source_batch_superseded(
        &self,
        scope: &Scope,
        source_id: &str,
        batch_key: &str,
        kind: SourceBatchRecordKind,
        record_id: &str,
    ) -> CoreResult<()> {
        self.require_scope(scope)?;
        let actor: String = "engram-batch".into();
        let item: String = record_id.to_owned();
        self.invoke(&actor, &item, |ports| async move {
            ports
                .knowledge
                .mark_source_batch_superseded(scope, source_id, batch_key, kind, record_id)
                .await
        })
        .await
    }
}

#[async_trait]
impl CompositionKnowledge for ObservedPorts {
    async fn put_source_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeSource,
    ) -> CoreResult<KnowledgeSource> {
        self.require_scope(expected)?;
        KnowledgeRepository::put_source(self, record).await
    }
    async fn put_document_in_scope(
        &self,
        expected: &Scope,
        record: SourceDocument,
    ) -> CoreResult<SourceDocument> {
        self.require_scope(expected)?;
        KnowledgeRepository::put_document(self, record).await
    }
    async fn put_chunk_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeChunk,
    ) -> CoreResult<KnowledgeChunk> {
        self.require_scope(expected)?;
        KnowledgeRepository::put_chunk(self, record).await
    }
    async fn put_entity_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeEntity,
    ) -> CoreResult<KnowledgeEntity> {
        self.require_scope(expected)?;
        KnowledgeRepository::put_entity(self, record).await
    }
    async fn put_relationship_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        self.require_scope(expected)?;
        KnowledgeRepository::put_relationship(self, record).await
    }
    async fn registry_source(&self, id: &SourceId) -> CoreResult<Option<KnowledgeSource>> {
        let actor: String = "engram-batch".into();
        let item: String = id.to_string();
        self.invoke(&actor, &item, |ports| async move {
            ports.knowledge.registry_source(id).await
        })
        .await
    }
}

#[async_trait]
impl crate::KnowledgeQuery for ObservedPorts {
    async fn list_entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        self.require_scope(scope)?;
        self.invoke("engram-batch", "entities", |ports| async move {
            crate::KnowledgeQuery::list_entities(ports.knowledge.as_ref(), scope).await
        })
        .await
    }
    async fn list_relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.require_scope(scope)?;
        self.invoke("engram-batch", "relationships", |ports| async move {
            crate::KnowledgeQuery::list_relationships(ports.knowledge.as_ref(), scope).await
        })
        .await
    }
    async fn list_chunks(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeChunk>> {
        self.require_scope(scope)?;
        self.invoke("engram-batch", "chunks", |ports| async move {
            crate::KnowledgeQuery::list_chunks(ports.knowledge.as_ref(), scope).await
        })
        .await
    }
    async fn list_graphs(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeGraph>> {
        self.require_scope(scope)?;
        self.invoke("engram-batch", "graphs", |ports| async move {
            crate::KnowledgeQuery::list_graphs(ports.knowledge.as_ref(), scope).await
        })
        .await
    }
    async fn list_chunks_by_document(
        &self,
        document_id: &DocumentId,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeChunk>> {
        self.require_scope(scope)?;
        self.invoke("engram-batch", "document-chunks", |ports| async move {
            crate::KnowledgeQuery::list_chunks_by_document(
                ports.knowledge.as_ref(),
                document_id,
                scope,
            )
            .await
        })
        .await
    }
}
