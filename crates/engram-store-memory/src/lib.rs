//! In-memory adapter for Engram memory services.
//!
//! This crate owns process-local storage used by specification tests, examples,
//! and first vertical slices. It implements core ports without making
//! `engram-core` depend on concrete state, clocks, ID counters, or storage
//! details. Durable SQL, vector, and provider-backed adapters should live in
//! separate crates and satisfy the same core contracts.

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
use chrono::Utc;
use engram_core::{
    Clock, CoreError, CoreResult, IdGenerator, MemoryEventRepository, MemoryRepository,
    MemoryService, PolicyAuthorizer,
};
use engram_domain::*;
use serde_json::json;

#[derive(Debug, Default)]
struct InMemoryState {
    memories: BTreeMap<String, MemoryRecord>,
    events: Vec<MemoryEvent>,
    idempotency: BTreeMap<String, WriteMemoryResponse>,
}

/// Clock implementation backed by the current system UTC time.
///
/// This is the default for local development. Tests that need exact timestamps
/// should inject a deterministic `Clock` through
/// `InMemoryMemoryService::with_dependencies`.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Utc::now()
    }
}

/// Monotonic process-local ID generator for in-memory adapters.
///
/// IDs are opaque contract values. The counter exists only to make local tests
/// deterministic and must not be treated as a storage ordering guarantee by
/// callers.
#[derive(Debug, Default)]
pub struct SequentialIdGenerator {
    value: AtomicU64,
}

impl SequentialIdGenerator {
    /// Creates a fresh process-local sequence for deterministic adapter IDs.
    ///
    /// The first emitted identifier ends in `000001`, but callers must still
    /// treat the full value as opaque. Tests may rely on the sequence for stable
    /// assertions; portable contracts must not infer ordering or scope from it.
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(1),
        }
    }
}

impl IdGenerator for SequentialIdGenerator {
    fn new_id(&self, entity_type: &'static str) -> Id {
        let value = self.value.fetch_add(1, Ordering::Relaxed);
        Id::from(format!("{entity_type}-{value:06}"))
    }
}

/// Policy authorizer that permits every operation.
///
/// This is a first-slice stub for tests and local development. Real deployments
/// should provide a stricter authorizer that understands requester roles,
/// permissions, visibility, retention, sensitivity, and allowed uses.
#[derive(Debug, Default)]
pub struct AllowAllPolicyAuthorizer;

impl PolicyAuthorizer for AllowAllPolicyAuthorizer {
    fn can_write(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_retrieve(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_forget(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }
}

/// In-memory implementation for the v1 write-memory slice.
///
/// This service supports writes, record lookup, lifecycle event lookup, and
/// status updates against process-local state. Retrieval and forgetting remain
/// spec-defined but intentionally unimplemented here so they can be built as
/// separate behavior slices.
#[derive(Clone)]
pub struct InMemoryMemoryService {
    state: Arc<Mutex<InMemoryState>>,
    authorizer: Arc<dyn PolicyAuthorizer>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}

impl InMemoryMemoryService {
    /// Creates an in-memory service with permissive policy and default local
    /// clock/ID dependencies.
    ///
    /// This constructor is meant for spec tests and examples. It still routes
    /// writes through core ports so stricter dependencies can be injected
    /// without changing write-path behavior.
    pub fn new() -> Self {
        Self::with_dependencies(
            Arc::new(AllowAllPolicyAuthorizer),
            Arc::new(SystemClock),
            Arc::new(SequentialIdGenerator::new()),
        )
    }

    /// Creates an in-memory service with an injected policy authorizer.
    ///
    /// Tests can use this to prove denied writes do not create memories or
    /// events while retaining the default local clock and ID generator.
    pub fn with_authorizer(authorizer: Arc<dyn PolicyAuthorizer>) -> Self {
        Self::with_dependencies(
            authorizer,
            Arc::new(SystemClock),
            Arc::new(SequentialIdGenerator::new()),
        )
    }

    /// Creates an in-memory service with all behavior dependencies injected.
    ///
    /// This constructor is the deterministic test seam for exact timestamps,
    /// stable identifiers, and policy outcomes. Production code should use a
    /// durable adapter rather than relying on this process-local store.
    pub fn with_dependencies(
        authorizer: Arc<dyn PolicyAuthorizer>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(InMemoryState::default())),
            authorizer,
            clock,
            ids,
        }
    }

    fn validate_write_request(request: &WriteMemoryRequest) -> CoreResult<()> {
        if request.scope.tenant.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "scope.tenant is required".to_owned(),
            });
        }
        if request.content.text.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "content.text is required".to_owned(),
            });
        }
        if request.provenance.source.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "provenance.source is required".to_owned(),
            });
        }
        Ok(())
    }

    fn idempotency_key(request: &WriteMemoryRequest) -> Option<String> {
        request.idempotency_key.as_ref().map(|key| {
            format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                request.scope.tenant,
                request.scope.subject.as_deref().unwrap_or_default(),
                request.scope.workspace.as_deref().unwrap_or_default(),
                key
            )
        })
    }

    fn lock_state(&self) -> CoreResult<std::sync::MutexGuard<'_, InMemoryState>> {
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
        Self::validate_write_request(&request)?;
        self.authorizer
            .can_write(&request.requester, &request.scope, &request.policy)?;

        let idempotency_key = request.idempotency_key.clone();
        let idempotency_lookup_key = Self::idempotency_key(&request);
        let now = self.clock.now();
        let memory_id = self.ids.new_id("memory");
        let event_id = self.ids.new_id("event");
        let record = MemoryRecord {
            id: memory_id.clone(),
            kind: request.kind,
            content: request.content,
            scope: request.scope.clone(),
            provenance: request.provenance.clone(),
            policy: request.policy,
            status: MemoryStatus::Active,
            links: request.links,
            assertions: Vec::new(),
            created_at: now,
            updated_at: None,
            metadata: None,
        };
        let event = MemoryEvent {
            id: event_id,
            kind: MemoryEventKind::Written,
            scope: request.scope,
            actor: request.requester.actor,
            memory_id: Some(memory_id),
            payload: idempotency_key
                .as_ref()
                .map_or_else(|| json!({}), |key| json!({ "idempotencyKey": key })),
            provenance: request.provenance,
            occurred_at: now,
            recorded_at: now,
        };
        let response = WriteMemoryResponse {
            record,
            event,
            deduplicated: Some(false),
        };

        let mut state = self.lock_state()?;
        if let Some(key) = &idempotency_lookup_key
            && let Some(existing) = state.idempotency.get(key)
        {
            let mut response = existing.clone();
            response.deduplicated = Some(true);
            return Ok(response);
        }

        state
            .memories
            .insert(response.record.id.to_string(), response.record.clone());
        state.events.push(response.event.clone());
        if let Some(key) = idempotency_lookup_key {
            state.idempotency.insert(key, response.clone());
        }
        Ok(response)
    }

    async fn retrieve(&self, _request: RetrievalRequest) -> CoreResult<ContextPayload> {
        Err(CoreError::InvalidRequest {
            reason: "retrieve is not implemented in the write-memory slice".to_owned(),
        })
    }

    async fn forget(&self, _request: ForgetRequest) -> CoreResult<ForgetResult> {
        Err(CoreError::InvalidRequest {
            reason: "forget is not implemented in the write-memory slice".to_owned(),
        })
    }
}

fn scope_allows(record_scope: &Scope, request_scope: &Scope) -> bool {
    record_scope.tenant == request_scope.tenant
        && optional_scope_matches(&record_scope.subject, &request_scope.subject)
        && optional_scope_matches(&record_scope.workspace, &request_scope.workspace)
        && optional_scope_matches(&record_scope.session, &request_scope.session)
        && optional_scope_matches(&record_scope.environment, &request_scope.environment)
}

fn optional_scope_matches(record_value: &Option<String>, request_value: &Option<String>) -> bool {
    request_value
        .as_ref()
        .is_none_or(|value| record_value.as_ref() == Some(value))
}
