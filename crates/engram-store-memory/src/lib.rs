//! In-memory adapter for Engram memory services.
//!
//! This crate owns process-local storage used by specification tests, examples,
//! and first vertical slices. It implements core ports without making
//! `engram-core` depend on concrete state, clocks, ID counters, or storage
//! details. Durable SQL, vector, and provider-backed adapters should live in
//! separate crates and satisfy the same core contracts.

use std::{
    collections::{BTreeMap, BTreeSet},
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
        if request
            .policy
            .allowed_uses
            .contains(&AllowedUse::TrainingExport)
        {
            return Err(CoreError::InvalidRequest {
                reason: "policy.allowedUses must not include training_export in v1".to_owned(),
            });
        }
        Ok(())
    }

    fn validate_retrieval_request(request: &RetrievalRequest) -> CoreResult<()> {
        if request.scope.tenant.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "scope.tenant is required".to_owned(),
            });
        }
        if request.query.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "query is required".to_owned(),
            });
        }
        if request.limit == Some(0) {
            return Err(CoreError::InvalidRequest {
                reason: "limit must be positive when supplied".to_owned(),
            });
        }
        if let Some(budget) = &request.budget
            && (budget.max_items == Some(0)
                || budget.max_tokens == Some(0)
                || budget.max_bytes == Some(0))
        {
            return Err(CoreError::InvalidRequest {
                reason: "budget limits must be positive when supplied".to_owned(),
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

    async fn retrieve(&self, request: RetrievalRequest) -> CoreResult<ContextPayload> {
        Self::validate_retrieval_request(&request)?;

        let now = self.clock.now();
        let terms = query_terms(&request.query);
        let include_explanations = request.include_explanations.unwrap_or(false);
        let max_items = effective_max_items(&request);
        let records = {
            let state = self.lock_state()?;
            state.memories.values().cloned().collect::<Vec<_>>()
        };

        let mut candidates = Vec::new();
        let mut omitted = Vec::new();
        for record in records {
            if !scope_allows(&record.scope, &request.scope) {
                continue;
            }
            if !memory_filter_allows(&record, request.filters.as_ref()) {
                continue;
            }
            if let Some(expires_at) = record.policy.expires_at
                && expires_at <= now
            {
                omitted.push(omitted_result(&record, OmittedReason::Expired));
                continue;
            }
            if matches!(
                record.status,
                MemoryStatus::Redacted | MemoryStatus::Forgotten
            ) {
                omitted.push(omitted_result(&record, OmittedReason::Redacted));
                continue;
            }
            if matches!(record.status, MemoryStatus::Archived)
                && !request
                    .filters
                    .as_ref()
                    .and_then(|filters| filters.include_archived)
                    .unwrap_or(false)
            {
                continue;
            }
            if !record.policy.allowed_uses.is_empty()
                && !record.policy.allowed_uses.contains(&AllowedUse::Retrieval)
            {
                omitted.push(omitted_result(&record, OmittedReason::PolicyDenied));
                continue;
            }
            if let Err(error) =
                self.authorizer
                    .can_retrieve(&request.requester, &record.scope, &record.policy)
            {
                if matches!(error, CoreError::PolicyDenied { .. }) {
                    omitted.push(omitted_result(&record, OmittedReason::PolicyDenied));
                    continue;
                }
                return Err(error);
            }

            if let Some((score, matched_terms)) = keyword_score(&record, &request.query, &terms) {
                candidates.push((score, matched_terms, record));
            }
        }

        candidates.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| right.2.created_at.cmp(&left.2.created_at))
                .then_with(|| left.2.id.cmp(&right.2.id))
        });

        let mut items = Vec::new();
        for (index, (score, matched_terms, record)) in candidates.into_iter().enumerate() {
            if index >= max_items {
                omitted.push(omitted_result(&record, OmittedReason::BudgetExceeded));
                continue;
            }
            items.push(retrieval_result(
                index,
                score,
                matched_terms,
                record,
                include_explanations,
            ));
        }

        Ok(ContextPayload {
            items,
            budget: request.budget,
            omitted,
            source_failures: Vec::new(),
            created_at: now,
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

fn effective_max_items(request: &RetrievalRequest) -> usize {
    let limit = request.limit.unwrap_or(u32::MAX);
    let budget_limit = request
        .budget
        .as_ref()
        .and_then(|budget| budget.max_items)
        .unwrap_or(u32::MAX);
    limit.min(budget_limit) as usize
}

fn memory_filter_allows(record: &MemoryRecord, filters: Option<&QueryFilter>) -> bool {
    let Some(filters) = filters else {
        return true;
    };
    if !filters.memory_kinds.is_empty() && !filters.memory_kinds.contains(&record.kind) {
        return false;
    }
    if let Some(since) = filters.since
        && record.created_at < since
    {
        return false;
    }
    if let Some(until) = filters.until
        && record.created_at > until
    {
        return false;
    }
    if let Some(min_confidence) = filters.min_confidence
        && record.provenance.confidence.unwrap_or(0.0) < min_confidence
    {
        return false;
    }
    true
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(|term| {
            let term = term.trim().to_lowercase();
            (!term.is_empty()).then_some(term)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn keyword_score(
    record: &MemoryRecord,
    query: &str,
    terms: &[String],
) -> Option<(f32, Vec<String>)> {
    let content = searchable_content(record);
    let normalized_query = query.trim().to_lowercase();
    let exact_match = content.contains(&normalized_query);
    let matched_terms = terms
        .iter()
        .filter(|term| content.contains(term.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    if !exact_match && matched_terms.is_empty() {
        return None;
    }

    let term_score = if terms.is_empty() {
        0.0
    } else {
        matched_terms.len() as f32 / terms.len() as f32
    };
    let relevance = if exact_match {
        1.0_f32.max(term_score)
    } else {
        term_score
    };
    let confidence = record.provenance.confidence.unwrap_or(1.0);
    let total = ((relevance * 0.85) + (confidence * 0.15)).min(1.0);

    Some((total, matched_terms))
}

fn searchable_content(record: &MemoryRecord) -> String {
    let mut content = record.content.text.to_lowercase();
    if let Some(summary) = &record.content.summary {
        content.push(' ');
        content.push_str(&summary.to_lowercase());
    }
    content
}

fn retrieval_result(
    index: usize,
    total_score: f32,
    matched_terms: Vec<String>,
    record: MemoryRecord,
    include_explanation: bool,
) -> RetrievalResult {
    let explanation = include_explanation.then(|| RetrievalExplanation {
        reason: "Matched memory content with in-memory keyword retrieval.".to_owned(),
        matched_cues: Vec::new(),
        matched_terms,
        path: Vec::new(),
        source_summary: record.content.summary.clone(),
    });
    RetrievalResult {
        id: format!("result-{}", record.id),
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        content: record.content.text,
        score: RetrievalScore {
            total: total_score,
            relevance: Some(total_score),
            recency: None,
            confidence: record.provenance.confidence,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: record.provenance,
        policy: record.policy,
        explanation,
        fusion_trace: Some(FusionTrace {
            source: "memory.keyword".to_owned(),
            source_rank: Some((index + 1) as u32),
            source_score: Some(total_score),
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(total_score),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(total_score),
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}

fn omitted_result(record: &MemoryRecord, reason: OmittedReason) -> OmittedResult {
    OmittedResult {
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        reason,
    }
}
