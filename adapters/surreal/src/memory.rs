//! Surreal memory cell — a `MemoryService` over embedded SurrealKV.
//!
//! Behavior mirrors the S7 stub backend (`tests/stub_backend.rs`) exactly so
//! the Surreal backend passes the same lifecycle the SQLite fixtures exercise;
//! only the persistence differs (SurrealKV tables instead of `HashMap`).
//!
//! ADR-0022: this module names `Surreal*` / holds the engine and is exempt from
//! the engine-neutrality gate.
//!
//! Connection lifecycle: the Surreal SDK requires a Tokio reactor, so the
//! connection is opened LAZILY on the first async method call (under the
//! consumer's runtime), not via sync `block_on` in `bootstrap_surreal`.

use std::sync::{Arc, Mutex};

use crate::SurrealConnection;
use async_trait::async_trait;
use engram_domain::{
    AllowedUse, DeleteMode, EventId, ForgetResult, ForgetStatus, ForgetTargetType, FusionStrategy,
    Id, MemoryContent, MemoryEvent, MemoryEventKind, MemoryId, MemoryRecord, MemoryStatus,
    OmittedReason, Policy, Provenance, Requester, RetrievalRequest, RetrievalResult,
    RetrievalScore, RetrievalTargetType, Scope, WriteMemoryRequest, WriteMemoryResponse,
};
use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryService};
use engram_runtime::{Clock, CoreError, CoreResult, IdGenerator, PolicyAuthorizer};
use serde::Deserialize;
use serde_json::json;

const MEMORY_TABLE: &str = "memory";
const EVENT_TABLE: &str = "memory_event";

// ---------------------------------------------------------------------------
// Engine-neutral behavior dependencies (local, mirroring the S7 stub so the
// closure stays engine-free). A future refactor could lift these into
// `engram-runtime` for reuse across SQLite / Surreal adapters.
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct SurrealClock;

impl Clock for SurrealClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }
}

#[derive(Debug, Default)]
struct SurrealIdGenerator {
    next: Mutex<u64>,
}

impl SurrealIdGenerator {
    fn new() -> Self {
        Self {
            next: Mutex::new(1),
        }
    }
}

impl IdGenerator for SurrealIdGenerator {
    fn new_id(&self, entity_type: &'static str) -> Id {
        let mut value = self.next.lock().expect("id generator poisoned");
        let n = *value;
        *value += 1;
        Id::from(format!("{entity_type}-{n:06}"))
    }
}

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

fn surreal_err(error: surrealdb::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "surreal.memory".to_owned(),
        message: error.to_string(),
    }
}

/// Deserialize shim for the `SELECT data FROM ...` pattern (each Surreal record
/// stores the full DTO under its `data` field to avoid collisions with Surreal's
/// own record metadata).
#[derive(Deserialize)]
struct DataWrapper<T> {
    data: T,
}

/// `MemoryService` backed by embedded SurrealKV.
pub struct SurrealMemoryService {
    conn: Arc<SurrealConnection>,
    authorizer: Arc<dyn PolicyAuthorizer>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}

impl SurrealMemoryService {
    /// Creates a memory service over a shared Surreal connection. The connection
    /// opens lazily on first use (under the consumer's Tokio runtime).
    pub fn new(conn: Arc<SurrealConnection>) -> Self {
        Self {
            conn,
            authorizer: Arc::new(AllowAllAuthorizer),
            clock: Arc::new(SurrealClock),
            ids: Arc::new(SurrealIdGenerator::new()),
        }
    }

    async fn list_records(&self) -> CoreResult<Vec<MemoryRecord>> {
        let db = self.conn.db().await?;
        let mut res = db
            .query(&format!("SELECT data FROM {MEMORY_TABLE}"))
            .await
            .map_err(surreal_err)?;
        let rows: Vec<DataWrapper<MemoryRecord>> = res.take(0).map_err(surreal_err)?;
        Ok(rows.into_iter().map(|w| w.data).collect())
    }

    async fn list_events(&self) -> CoreResult<Vec<MemoryEvent>> {
        let db = self.conn.db().await?;
        let mut res = db
            .query(&format!("SELECT data FROM {EVENT_TABLE}"))
            .await
            .map_err(surreal_err)?;
        let rows: Vec<DataWrapper<MemoryEvent>> = res.take(0).map_err(surreal_err)?;
        Ok(rows.into_iter().map(|w| w.data).collect())
    }

    async fn remove_memory(&self, id: &MemoryId) -> CoreResult<()> {
        let db = self.conn.db().await?;
        db.query(&format!("DELETE type::thing('{MEMORY_TABLE}', $key)"))
            .bind(("key", id.to_string()))
            .await
            .map_err(surreal_err)?;
        Ok(())
    }
}

#[async_trait]
impl MemoryRepository for SurrealMemoryService {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        let db = self.conn.db().await?;
        let key = record.id.to_string();
        db.query(&format!(
            "UPSERT type::thing('{MEMORY_TABLE}', $key) SET data = $record"
        ))
        .bind(("key", key))
        .bind(("record", record.clone()))
        .await
        .map_err(surreal_err)?;
        Ok(record)
    }

    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        let db = self.conn.db().await?;
        let mut res = db
            .query(&format!(
                "SELECT data FROM type::thing('{MEMORY_TABLE}', $key)"
            ))
            .bind(("key", id.to_string()))
            .await
            .map_err(surreal_err)?;
        let rows: Vec<DataWrapper<MemoryRecord>> = res.take(0).map_err(surreal_err)?;
        Ok(rows
            .into_iter()
            .next()
            .filter(|w| scope_allows(&w.data.scope, scope))
            .map(|w| w.data))
    }

    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent> {
        let db = self.conn.db().await?;
        let key = event.id.to_string();
        db.query(&format!(
            "UPSERT type::thing('{EVENT_TABLE}', $key) SET data = $event"
        ))
        .bind(("key", key))
        .bind(("event", event.clone()))
        .await
        .map_err(surreal_err)?;
        Ok(event)
    }

    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        let Some(mut record) = self.get_memory(id, scope).await? else {
            return Err(CoreError::NotFound {
                target_type: "memory",
                target_id: id.to_string(),
            });
        };
        record.status = status;
        self.put_memory(record).await
    }
}

#[async_trait]
impl MemoryEventRepository for SurrealMemoryService {
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        let db = self.conn.db().await?;
        let mut res = db
            .query(&format!(
                "SELECT data FROM type::thing('{EVENT_TABLE}', $key)"
            ))
            .bind(("key", id.to_string()))
            .await
            .map_err(surreal_err)?;
        let rows: Vec<DataWrapper<MemoryEvent>> = res.take(0).map_err(surreal_err)?;
        Ok(rows
            .into_iter()
            .next()
            .filter(|w| scope_allows(&w.data.scope, scope))
            .map(|w| w.data))
    }

    async fn list_events_for_memory(
        &self,
        memory_id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        Ok(self
            .list_events()
            .await?
            .into_iter()
            .filter(|event| event.memory_id.as_ref() == Some(memory_id))
            .filter(|event| scope_allows(&event.scope, scope))
            .collect())
    }

    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        Ok(self
            .list_events()
            .await?
            .into_iter()
            .filter(|event| scope_allows(&event.scope, scope))
            .collect())
    }
}

#[async_trait]
impl MemoryService for SurrealMemoryService {
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

        let records = self.list_records().await?;
        let normalized_query = request.query.trim().to_lowercase();

        let mut candidates: Vec<(f32, MemoryRecord)> = Vec::new();
        let mut omitted = Vec::new();

        for record in records {
            if !scope_allows(&record.scope, &request.scope) {
                continue;
            }
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

            let hay = searchable(&record);
            let matched = normalized_query.is_empty() || hay.contains(&normalized_query);
            if !matched {
                continue;
            }
            let confidence = record.provenance.confidence.unwrap_or(1.0);
            let score = (0.85_f32 + (confidence * 0.15)).min(1.0);
            candidates.push((score, record));
        }

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
        apply_forget(self, request).await
    }
}

// `forget` logic factored out so the trait method delegates cleanly.
async fn apply_forget(
    svc: &SurrealMemoryService,
    request: engram_domain::ForgetRequest,
) -> CoreResult<ForgetResult> {
    if request.target_type != ForgetTargetType::Memory {
        return Err(CoreError::InvalidRequest {
            reason: "only memory forget targets are implemented".to_owned(),
        });
    }

    let memory_id = MemoryId::from(request.target_id.clone());
    let Some(existing) = svc.get_memory(&memory_id, &request.scope).await? else {
        return Ok(ForgetResult {
            target_type: "memory".to_owned(),
            target_id: request.target_id,
            status: ForgetStatus::NotFound,
            event: None,
        });
    };
    svc.authorizer
        .can_forget(&request.requester, &existing.scope, &existing.policy)?;

    let now = svc.clock.now();
    let mode_name = match request.mode {
        DeleteMode::Delete => "delete",
        DeleteMode::Redact => "redact",
        DeleteMode::Tombstone => "tombstone",
        DeleteMode::Archive => "archive",
    };
    let event = MemoryEvent {
        id: svc.ids.new_id("event"),
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

    match request.mode {
        DeleteMode::Delete => {
            svc.remove_memory(&existing.id).await?;
        }
        DeleteMode::Redact => {
            let mut record = existing.clone();
            record.status = MemoryStatus::Redacted;
            record.content.text.clear();
            record.content.summary = None;
            record.content.entities.clear();
            record.content.structured = None;
            record.links.clear();
            record.assertions.clear();
            record.updated_at = Some(now);
            svc.put_memory(record).await?;
        }
        DeleteMode::Tombstone => {
            let mut record = existing.clone();
            record.status = MemoryStatus::Forgotten;
            record.content.text.clear();
            record.content.summary = None;
            record.content.structured = None;
            record.updated_at = Some(now);
            svc.put_memory(record).await?;
        }
        DeleteMode::Archive => {
            let mut record = existing.clone();
            record.status = MemoryStatus::Archived;
            record.updated_at = Some(now);
            svc.put_memory(record).await?;
        }
    }
    let event = svc.append_event(event).await?;

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

fn searchable(record: &MemoryRecord) -> String {
    let mut content = record.content.text.to_lowercase();
    if let Some(summary) = &record.content.summary {
        content.push(' ');
        content.push_str(&summary.to_lowercase());
    }
    content
}

#[allow(clippy::too_many_arguments)]
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
            source: "surreal.memory.keyword".to_owned(),
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
