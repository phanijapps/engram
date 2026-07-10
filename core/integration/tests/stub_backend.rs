//! S7 — Backend-parametric conformance proof (ADR-0022 capstone).
//!
//! ## The contract this file proves
//!
//! ADR-0022 makes a two-part promise: *"swapping the storage backend is a
//! config/crate change, not an application rewrite."* That promise is only as
//! strong as its two halves, and this file is the second half:
//!
//! 1. **Neutrality gate (S1, `check-engine-neutrality.sh`).** Enforces that the
//!    port-trait crates (`engram-domain`, `engram-memory`, `engram-knowledge`,
//!    `engram-retrieval`, `engram-belief`, `engram-hierarchy`,
//!    `engram-consolidation`, `engram-orchestration`) name ZERO engine symbols
//!    — no `Sql*`, no `rusqlite`, no raw-SQL literals. The port layer is
//!    engine-symbol-free by construction; the gate keeps it honest.
//!
//! 2. **Stub backend (S7, this file).** Proves the traits declared by those
//!    neutral crates are *satisfiable* without SQLite. `StubMemoryService`
//!    implements the full `MemoryService` trait (`MemoryRepository` +
//!    `MemoryEventRepository` + the three service operations) backed only by
//!    `std::collections::HashMap`. It round-trips write -> retrieve -> forget
//!    with scope isolation, through the trait interface — exactly the lifecycle
//!    the SQLite fixtures exercise.
//!
//! Together: the port layer is engine-symbol-free (gate-enforced) **and** a
//! non-SQLite backend satisfies the traits (stub-proven). Therefore a backend
//! swap is configuration, not a rewrite. Removing either half reopens the
//! contract.
//!
//! ## Why this is a compile-time guarantee, not just a convention
//!
//! This test target lives in `engram-integration`, whose `Cargo.toml` depends
//! on `engram-memory` (the trait) and `engram-domain`/`engram-runtime` (the
//! types) but **does NOT depend on `engram-store-sql`**. The `Sql*` types are
//! not merely avoided by the stub — they are absent from this target's
//! dependency graph, so they cannot be named even by accident. The
//! `stub_names_zero_engine_types` test below reads this file's own source and
//! asserts no `Sql*` / `rusqlite` / `sqlite` tokens appear, making the proof
//! visible in test output rather than implicit in the build graph.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::{
    Actor, ActorKind, AllowedUse, DeleteMode, EventId, ForgetResult, ForgetStatus,
    ForgetTargetType, FusionStrategy, Id, MemoryContent, MemoryEvent, MemoryEventKind, MemoryId,
    MemoryKind, MemoryRecord, MemoryStatus, OmittedReason, Policy, Provenance, Requester,
    Retention, RetrievalRequest, RetrievalResult, RetrievalScore, RetrievalTargetType, Scope,
    Sensitivity, Visibility, WriteMemoryRequest, WriteMemoryResponse,
};
use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryService};
use engram_runtime::{Clock, CoreError, CoreResult, IdGenerator, PolicyAuthorizer};
use futures::executor::block_on;
use serde_json::json;

// ---------------------------------------------------------------------------
// Engine-neutral dependencies (local, stdlib-only implementations).
//
// These mirror the role of `engram-store-sql`'s `SystemClock`,
// `SequentialIdGenerator`, and `AllowAllPolicyAuthorizer` without referencing
// them. They are defined here so the stub's entire closure is engine-free.
// ---------------------------------------------------------------------------

/// UTC system clock — the stub's time source.
#[derive(Debug, Default)]
struct StubClock;

impl Clock for StubClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        Utc::now()
    }
}

/// Monotonic integer-suffixed ID generator for deterministic test identifiers.
#[derive(Debug, Default)]
struct StubIdGenerator {
    next: Mutex<u64>,
}

impl StubIdGenerator {
    fn new() -> Self {
        Self {
            next: Mutex::new(1),
        }
    }
}

impl IdGenerator for StubIdGenerator {
    fn new_id(&self, entity_type: &'static str) -> Id {
        let mut value = self.next.lock().expect("id generator poisoned");
        let n = *value;
        *value += 1;
        Id::from(format!("{entity_type}-{n:06}"))
    }
}

/// Permissive authorizer — matches the conformance fixture posture.
#[derive(Debug, Default)]
struct AllowAllAuthorizer;

impl PolicyAuthorizer for AllowAllAuthorizer {
    fn can_write(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
    fn can_retrieve(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
    fn can_forget(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Scope visibility — mirrors `engram-store-sql::scope::scope_allows` without
// referencing it. Tenant must match exactly; optional request fields narrow.
// ---------------------------------------------------------------------------

fn scope_allows(record_scope: &Scope, request_scope: &Scope) -> bool {
    record_scope.tenant == request_scope.tenant
        && optional_matches(&record_scope.subject, &request_scope.subject)
        && optional_matches(&record_scope.workspace, &request_scope.workspace)
        && optional_matches(&record_scope.session, &request_scope.session)
        && optional_matches(&record_scope.environment, &request_scope.environment)
}

fn optional_matches(record: &Option<String>, request: &Option<String>) -> bool {
    request
        .as_ref()
        .is_none_or(|value| record.as_ref() == Some(value))
}

// ---------------------------------------------------------------------------
// StubMemoryService — HashMap-backed MemoryService.
//
// This is the proof artifact: a complete `MemoryService` implementation whose
// storage is `HashMap<MemoryId, MemoryRecord>` + a `Vec<MemoryEvent>`. It
// names ZERO `Sql*` types. It is a test artifact, never a production adapter.
// ---------------------------------------------------------------------------

/// Non-SQLite memory service backed by stdlib maps.
///
/// Implements the full `MemoryService` trait surface so the same lifecycle the
/// SQLite fixtures exercise (write -> retrieve -> forget, scope isolation) runs
/// against an engine-free backend, proving the port is backend-parametric.
pub struct StubMemoryService {
    records: Mutex<HashMap<MemoryId, MemoryRecord>>,
    events: Mutex<Vec<MemoryEvent>>,
    authorizer: Arc<dyn PolicyAuthorizer>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}

impl StubMemoryService {
    /// Creates an empty stub service with permissive default dependencies.
    pub fn new() -> Self {
        Self::with_dependencies(
            Arc::new(AllowAllAuthorizer),
            Arc::new(StubClock),
            Arc::new(StubIdGenerator::new()),
        )
    }

    /// Creates a stub service with explicit behavior dependencies, mirroring
    /// `SqlMemoryService::with_dependencies` for symmetry of construction.
    pub fn with_dependencies(
        authorizer: Arc<dyn PolicyAuthorizer>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            events: Mutex::new(Vec::new()),
            authorizer,
            clock,
            ids,
        }
    }
}

impl Default for StubMemoryService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryRepository for StubMemoryService {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        let mut records = self.records.lock().map_err(poisoned)?;
        records.insert(record.id.clone(), record.clone());
        Ok(record)
    }

    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        let records = self.records.lock().map_err(poisoned)?;
        Ok(records
            .get(id)
            .filter(|record| scope_allows(&record.scope, scope))
            .cloned())
    }

    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent> {
        let mut events = self.events.lock().map_err(poisoned)?;
        events.push(event.clone());
        Ok(event)
    }

    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        let mut records = self.records.lock().map_err(poisoned)?;
        let record = records
            .get_mut(id)
            .filter(|record| scope_allows(&record.scope, scope))
            .ok_or_else(|| CoreError::NotFound {
                target_type: "memory",
                target_id: id.to_string(),
            })?;
        record.status = status;
        Ok(record.clone())
    }
}

#[async_trait]
impl MemoryEventRepository for StubMemoryService {
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        let events = self.events.lock().map_err(poisoned)?;
        Ok(events
            .iter()
            .find(|event| &event.id == id)
            .filter(|event| scope_allows(&event.scope, scope))
            .cloned())
    }

    async fn list_events_for_memory(
        &self,
        memory_id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        let events = self.events.lock().map_err(poisoned)?;
        Ok(events
            .iter()
            .filter(|event| event.memory_id.as_ref() == Some(memory_id))
            .filter(|event| scope_allows(&event.scope, scope))
            .cloned()
            .collect())
    }

    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        let events = self.events.lock().map_err(poisoned)?;
        Ok(events
            .iter()
            .filter(|event| scope_allows(&event.scope, scope))
            .cloned()
            .collect())
    }
}

#[async_trait]
impl MemoryService for StubMemoryService {
    async fn write_memory(&self, request: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse> {
        if request.content.text.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "memory content text must not be empty".to_owned(),
            });
        }
        self.authorizer
            .can_write(&request.requester, &request.scope, &request.policy)?;

        let now = self.clock.now();
        let memory_id = self.ids.new_id("memory");

        // Enrich content.entities with extracted cue anchors, mirroring the
        // SQLite write path (`engram_memory::extract` + `merge_entities`).
        let content = {
            let extracted = engram_memory::extract(&request.content.text);
            let entities = engram_memory::merge_entities(extracted, request.content.entities);
            MemoryContent {
                entities,
                ..request.content
            }
        };

        let record = MemoryRecord {
            id: memory_id.clone(),
            kind: request.kind,
            content,
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
            id: self.ids.new_id("event"),
            kind: MemoryEventKind::Written,
            scope: request.scope,
            actor: request.requester.actor,
            memory_id: Some(memory_id),
            payload: json!({}),
            provenance: record.provenance.clone(),
            occurred_at: now,
            recorded_at: now,
        };

        // Store record then append the lifecycle event.
        self.put_memory(record.clone()).await?;
        let event = self.append_event(event).await?;

        Ok(WriteMemoryResponse {
            record,
            event,
            deduplicated: None,
        })
    }

    async fn retrieve(
        &self,
        request: RetrievalRequest,
    ) -> CoreResult<engram_domain::ContextPayload> {
        let now = self.clock.now();
        let limit = request.limit.unwrap_or(u32::MAX) as usize;

        let records = self.records.lock().map_err(poisoned)?;
        let normalized_query = request.query.trim().to_lowercase();

        let mut candidates: Vec<(f32, MemoryRecord)> = Vec::new();
        let mut omitted = Vec::new();

        for record in records.values() {
            // Scope boundary first — cross-tenant records are invisible.
            if !scope_allows(&record.scope, &request.scope) {
                continue;
            }
            // Lifecycle gauntlet: redacted/forgotten records are omitted, not
            // returned, mirroring the SQLite retrieval policy.
            if matches!(
                record.status,
                MemoryStatus::Redacted | MemoryStatus::Forgotten
            ) {
                omitted.push(engram_domain::OmittedResult {
                    target_type: RetrievalTargetType::Memory,
                    target_id: record.id.to_string(),
                    reason: OmittedReason::Redacted,
                });
                continue;
            }
            if matches!(record.status, MemoryStatus::Archived)
                && !request
                    .filters
                    .as_ref()
                    .and_then(|f| f.include_archived)
                    .unwrap_or(false)
            {
                continue;
            }
            if !record.policy.allowed_uses.is_empty()
                && !record.policy.allowed_uses.contains(&AllowedUse::Retrieval)
            {
                omitted.push(engram_domain::OmittedResult {
                    target_type: RetrievalTargetType::Memory,
                    target_id: record.id.to_string(),
                    reason: OmittedReason::PolicyDenied,
                });
                continue;
            }
            if record
                .policy
                .expires_at
                .is_some_and(|expires_at| expires_at <= now)
            {
                omitted.push(engram_domain::OmittedResult {
                    target_type: RetrievalTargetType::Memory,
                    target_id: record.id.to_string(),
                    reason: OmittedReason::Expired,
                });
                continue;
            }

            // Keyword match (substring on text + summary), the baseline the
            // SQLite keyword mode also reduces to.
            let hay = searchable(record);
            let matched = normalized_query.is_empty() || hay.contains(&normalized_query);
            if !matched {
                continue;
            }
            let confidence = record.provenance.confidence.unwrap_or(1.0);
            let score = (0.85_f32 + (confidence * 0.15)).min(1.0);
            candidates.push((score, record.clone()));
        }

        // Deterministic ordering: score desc, then created_at desc, then id.
        candidates.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| b.1.created_at.cmp(&a.1.created_at))
                .then_with(|| a.1.id.cmp(&b.1.id))
        });

        let mut items = Vec::new();
        for (index, (score, record)) in candidates.into_iter().enumerate() {
            if index >= limit {
                omitted.push(engram_domain::OmittedResult {
                    target_type: RetrievalTargetType::Memory,
                    target_id: record.id.to_string(),
                    reason: OmittedReason::BudgetExceeded,
                });
                continue;
            }
            items.push(build_result(index, score, record));
        }

        Ok(engram_domain::ContextPayload {
            items,
            budget: request.budget,
            omitted,
            source_failures: Vec::new(),
            created_at: now,
        })
    }

    async fn forget(&self, request: engram_domain::ForgetRequest) -> CoreResult<ForgetResult> {
        if request.target_type != ForgetTargetType::Memory {
            return Err(CoreError::InvalidRequest {
                reason: "only memory forget targets are implemented".to_owned(),
            });
        }

        let memory_id = MemoryId::from(request.target_id.clone());
        let Some(existing) = self.get_memory(&memory_id, &request.scope).await? else {
            return Ok(ForgetResult {
                target_type: "memory".to_owned(),
                target_id: request.target_id,
                status: ForgetStatus::NotFound,
                event: None,
            });
        };
        self.authorizer
            .can_forget(&request.requester, &existing.scope, &existing.policy)?;

        let now = self.clock.now();
        let mode_name = match request.mode {
            DeleteMode::Delete => "delete",
            DeleteMode::Redact => "redact",
            DeleteMode::Tombstone => "tombstone",
            DeleteMode::Archive => "archive",
        };
        let event = MemoryEvent {
            id: self.ids.new_id("event"),
            kind: match request.mode {
                DeleteMode::Redact => MemoryEventKind::Redacted,
                _ => MemoryEventKind::Forgotten,
            },
            scope: existing.scope.clone(),
            actor: request.requester.actor.clone(),
            memory_id: Some(existing.id.clone()),
            payload: json!({ "mode": mode_name, "reason": request.reason }),
            provenance: Provenance {
                source: "forget_request".to_owned(),
                actor: request.requester.actor,
                observed_at: now,
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: Some("manual".to_owned()),
            },
            occurred_at: now,
            recorded_at: now,
        };

        // Apply the lifecycle mode to the stored record.
        {
            let mut records = self.records.lock().map_err(poisoned)?;
            match request.mode {
                DeleteMode::Delete => {
                    records.remove(&existing.id);
                }
                DeleteMode::Redact => {
                    if let Some(record) = records.get_mut(&existing.id) {
                        record.status = MemoryStatus::Redacted;
                        record.content.text.clear();
                        record.content.summary = None;
                        record.content.entities.clear();
                        record.content.structured = None;
                        record.links.clear();
                        record.assertions.clear();
                        record.updated_at = Some(now);
                    }
                }
                DeleteMode::Tombstone => {
                    if let Some(record) = records.get_mut(&existing.id) {
                        record.status = MemoryStatus::Forgotten;
                        record.content.text.clear();
                        record.content.summary = None;
                        record.content.structured = None;
                        record.updated_at = Some(now);
                    }
                }
                DeleteMode::Archive => {
                    if let Some(record) = records.get_mut(&existing.id) {
                        record.status = MemoryStatus::Archived;
                        record.updated_at = Some(now);
                    }
                }
            }
        }
        let event = self.append_event(event).await?;

        let status = match request.mode {
            DeleteMode::Delete => ForgetStatus::Deleted,
            DeleteMode::Redact => ForgetStatus::Redacted,
            DeleteMode::Tombstone => ForgetStatus::Tombstoned,
            DeleteMode::Archive => ForgetStatus::Archived,
        };
        Ok(ForgetResult {
            target_type: "memory".to_owned(),
            target_id: existing.id.to_string(),
            status,
            event: Some(event),
        })
    }
}

fn poisoned<T>(_: T) -> CoreError {
    CoreError::Adapter {
        adapter: "stub.memory".to_owned(),
        message: "interior lock poisoned".to_owned(),
    }
}

fn searchable(record: &MemoryRecord) -> String {
    let mut content = record.content.text.to_lowercase();
    if let Some(summary) = &record.content.summary {
        content.push(' ');
        content.push_str(&summary.to_lowercase());
    }
    content
}

fn build_result(index: usize, score: f32, record: MemoryRecord) -> RetrievalResult {
    RetrievalResult {
        id: format!("result-{}", record.id),
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        content: record.content.text,
        score: RetrievalScore {
            total: score,
            relevance: Some(score),
            recency: None,
            confidence: record.provenance.confidence,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: record.provenance,
        policy: record.policy,
        explanation: None,
        fusion_trace: Some(engram_domain::FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: "stub.memory.keyword".to_owned(),
            source_rank: Some((index + 1) as u32),
            source_score: Some(score),
            score: None,
            rank: None,
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(score),
            rerank_strategy: None,
            rerank_score: None,
            discard_reason: None,
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}

// ---------------------------------------------------------------------------
// Domain constructors — mirror `adapters/integration/src/fixtures/support.rs`
// but local so the stub's closure stays inside this single engine-free file.
// ---------------------------------------------------------------------------

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: Some("subject-a".to_owned()),
        workspace: Some("workspace-a".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn requester() -> Requester {
    Requester {
        actor: Actor {
            id: Id::from("conformance-agent"),
            kind: ActorKind::Agent,
            display_name: Some("Conformance".to_owned()),
            metadata: None,
        },
        roles: Vec::new(),
        permissions: vec!["memory.write".to_owned(), "memory.retrieve".to_owned()],
        on_behalf_of: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "conformance".to_owned(),
        actor: Actor {
            id: Id::from("conformance-agent"),
            kind: ActorKind::Agent,
            display_name: Some("Conformance Harness".to_owned()),
            metadata: None,
        },
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Medium),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn write_request(tenant: &str) -> WriteMemoryRequest {
    WriteMemoryRequest {
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: "conformance memory".to_owned(),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope(tenant),
        requester: requester(),
        provenance: provenance(),
        policy: policy(),
        links: Vec::new(),
        idempotency_key: None,
    }
}

fn retrieve_request(tenant: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: "conformance".to_owned(),
        scope: scope(tenant),
        requester: requester(),
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: None,
    }
}

fn forget_request(id: &str, tenant: &str) -> engram_domain::ForgetRequest {
    engram_domain::ForgetRequest {
        target_type: ForgetTargetType::Memory,
        target_id: id.to_owned(),
        scope: scope(tenant),
        requester: requester(),
        mode: DeleteMode::Tombstone,
        reason: None,
    }
}

// ===========================================================================
// Tests
// ===========================================================================

/// T1 core: the stub MemoryService round-trips write -> retrieve -> forget with
/// scope isolation, driven entirely through the `MemoryService` trait. This is
/// the same lifecycle `adapters/integration` exercises against `SqlMemoryService`
/// — here it runs against a backend that names zero `Sql*` types.
#[test]
fn stub_backend_round_trips_memory_lifecycle_with_scope_isolation() {
    // Drive the lifecycle through the trait interface so the proof exercises
    // the port, not the concrete struct.
    let service: Box<dyn MemoryService> = Box::new(StubMemoryService::new());

    // Write in tenant-a.
    let stored = block_on(service.write_memory(write_request("tenant-a")))
        .expect("write_memory against stub backend must succeed");
    let memory_id = stored.record.id.to_string();
    assert_eq!(stored.record.status, MemoryStatus::Active);
    assert_eq!(stored.event.kind, MemoryEventKind::Written);

    // Retrieve in tenant-a succeeds.
    let visible = block_on(service.retrieve(retrieve_request("tenant-a")))
        .expect("retrieve in tenant-a must succeed");
    let found_a = visible
        .items
        .iter()
        .any(|r| r.target_id == memory_id && r.target_type == RetrievalTargetType::Memory);
    assert!(
        found_a,
        "tenant-a must see its own memory through the stub backend"
    );

    // Scope isolation: tenant-b must not see tenant-a's memory.
    let hidden = block_on(service.retrieve(retrieve_request("tenant-b")))
        .expect("retrieve in tenant-b must succeed (empty result)");
    let leaked = hidden.items.iter().any(|r| r.target_id == memory_id);
    assert!(
        !leaked,
        "stub backend must not leak memories across tenants"
    );

    // Forget in tenant-a tombstones the memory.
    let forgotten = block_on(service.forget(forget_request(&memory_id, "tenant-a")))
        .expect("forget against stub backend must succeed");
    assert_eq!(forgotten.status, ForgetStatus::Tombstoned);
    assert!(forgotten.event.is_some(), "forget must record an event");

    // After tombstone, retrieve in tenant-a omits the memory.
    let after = block_on(service.retrieve(retrieve_request("tenant-a")))
        .expect("retrieve after forget must succeed");
    let still_visible = after.items.iter().any(|r| r.target_id == memory_id);
    assert!(
        !still_visible,
        "tombstoned memory must not appear in retrieval results"
    );
    assert!(
        after
            .omitted
            .iter()
            .any(|o| o.target_id == memory_id && o.reason == OmittedReason::Redacted),
        "tombstoned memory should be reported as omitted/redacted"
    );
}

/// T1 guarantee: the stub's own source names zero engine types. This reads the
/// file at compile time (`include_str!`) and rejects any `Sql*` / `rusqlite` /
/// `sqlite` token, making the engine-neutrality proof explicit in test output
/// rather than implicit in the build graph.
#[test]
fn stub_names_zero_engine_types() {
    let source = include_str!("stub_backend.rs");

    let engine_token = regex_lite_engine_token(source);
    assert!(
        engine_token.is_none(),
        "ADR-0022 backend-parametric proof violated: stub source names an \
         engine token ({engine_token:?}). The stub must name zero Sql*/rusqlite/sqlite types.",
    );

    // Sanity: the stub really does implement the trait.
    let _: &dyn MemoryService = &StubMemoryService::new();
}

/// Returns the first engine-symbol match found in `source`, or `None`.
///
/// Mirrors the forbidden-type class from `check-engine-neutrality.sh`
/// (`\bSql[A-Z]...`) plus the bare engine-crate literals, scoped to identifier
/// usages on code lines. Comment lines (`//!`, `//`, attributes) and string
/// literals are stripped before scanning: the contract proof is allowed to
/// *discuss* the forbidden symbols by name in prose — what it must not do is
/// *use* them as types. Stripping strings keeps this self-test honest about
/// that distinction.
fn regex_lite_engine_token(source: &str) -> Option<String> {
    // Strip string literals across the WHOLE source first, so multi-line
    // literals (line-continued assertion messages) are blanked consistently.
    let stripped = strip_string_literals(source);
    for (lineno, raw) in stripped.lines().enumerate() {
        // Skip comment and attribute lines — the proof is allowed to mention
        // engine symbols in documentation. The violation is a *type usage* on
        // a code line, which `check-engine-neutrality.sh` also keys on.
        let trimmed = raw.trim_start();
        if trimmed.starts_with("//!") || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        for token in raw.split(|c: char| !c.is_alphanumeric() && c != '_') {
            if token.starts_with("Sql")
                && token.len() > 3
                && token.as_bytes()[3].is_ascii_uppercase()
            {
                return Some(format!("line {}: {}", lineno + 1, token));
            }
            if token == "rusqlite" || token == "sqlite" || token == "sqlite_vec" {
                return Some(format!("line {}: {}", lineno + 1, token));
            }
        }
    }
    None
}

/// Replaces the contents of `"..."` string literals with spaces across the
/// entire source, preserving layout so line numbers stay meaningful. Tracks
/// state across newlines so multi-line (line-continued) literals are handled.
/// It is intentionally simple — this guards a self-test, not production
/// parsing; raw strings are not used by the stub under test.
fn strip_string_literals(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_string = false;
    let mut escaped = false;
    for ch in source.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
                out.push('"');
                continue;
            }
            // Preserve newlines so line numbering is stable; blank other
            // literal content.
            out.push(if ch == '\n' { '\n' } else { ' ' });
        } else if ch == '"' {
            in_string = true;
            out.push('"');
        } else {
            out.push(ch);
        }
    }
    out
}

/// T1 supplementary: forget on a non-existent memory returns NotFound (not an
/// error), and cross-tenant forget is a no-op for the caller's scope.
#[test]
fn stub_forget_reports_not_found_and_respects_scope() {
    let service = StubMemoryService::new();

    // Forget a memory id that was never written.
    let result = block_on(service.forget(forget_request("memory-does-not-exist", "tenant-a")))
        .expect("forget of missing memory must not error");
    assert_eq!(result.status, ForgetStatus::NotFound);
    assert!(result.event.is_none());

    // Write in tenant-a, attempt forget from tenant-b (wrong scope).
    let stored = block_on(service.write_memory(write_request("tenant-a")))
        .expect("write_memory must succeed");
    let memory_id = stored.record.id.to_string();

    let cross = block_on(service.forget(forget_request(&memory_id, "tenant-b")))
        .expect("cross-tenant forget must not error");
    assert_eq!(
        cross.status,
        ForgetStatus::NotFound,
        "tenant-b must not find tenant-a's memory through forget"
    );

    // The memory is still present for tenant-a.
    let visible =
        block_on(service.retrieve(retrieve_request("tenant-a"))).expect("retrieve must succeed");
    assert!(
        visible.items.iter().any(|r| r.target_id == memory_id),
        "cross-tenant forget must not delete tenant-a's memory"
    );
}

/// T1 supplementary: the stub preserves lifecycle events through the event
/// repository trait, so the audit surface is backend-parametric too.
#[test]
fn stub_records_lifecycle_events() {
    let service = StubMemoryService::new();
    let stored = block_on(service.write_memory(write_request("tenant-a")))
        .expect("write_memory must succeed");
    let memory_id = stored.record.id.clone();

    let events = block_on(service.list_events_for_memory(&memory_id, &scope("tenant-a")))
        .expect("list_events_for_memory must succeed");
    assert_eq!(events.len(), 1, "write should append exactly one event");
    assert_eq!(events[0].kind, MemoryEventKind::Written);

    // Cross-scope event listing is empty.
    let hidden = block_on(service.list_events_for_memory(&memory_id, &scope("tenant-b")))
        .expect("cross-scope list must succeed");
    assert!(hidden.is_empty(), "tenant-b must not see tenant-a's events");
}
