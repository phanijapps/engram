//! P2 cells: MemoryService, BeliefRepository, HierarchyRepository,
//! ProcedureRepository over Postgres.
//!
//! All use the same JSONB pattern: put/get/delete/list by (table, id, scope).

use async_trait::async_trait;
use engram_domain::operations::{
    ForgetRequest, ForgetResult, ForgetStatus, WriteMemoryRequest, WriteMemoryResponse,
};
use engram_domain::*;
use engram_memory::MemoryRepository;
use engram_runtime::{CoreError, CoreResult};
use serde_json::{self, json};

use crate::connection::PgConnection;

fn pg_err(e: String) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-pgvector".into(),
        message: e,
    }
}

#[allow(dead_code)]
fn pg_res<T, E: std::fmt::Display>(r: Result<T, E>) -> CoreResult<T> {
    r.map_err(|e| pg_err(e.to_string()))
}

// === Generic JSONB helpers (shared by all cells) ===

fn put_scoped(
    conn: &PgConnection,
    table: &str,
    id: &str,
    json: serde_json::Value,
    s: &Scope,
) -> CoreResult<()> {
    conn.block_on(async {
        conn.client.execute(
            &format!("INSERT INTO {table} (id, record_json, tenant, subject, workspace, session, environment) \
                      VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (id) DO UPDATE SET \
                      record_json=EXCLUDED.record_json, last_updated_at=now()"),
            &[&id, &json, &s.tenant, &s.subject, &s.workspace, &s.session, &s.environment],
        ).await.map_err(|e| pg_err(e.to_string()))?;
        Ok::<_, CoreError>(())
    })
}

fn get_scoped<T: serde::de::DeserializeOwned + Send>(
    conn: &PgConnection,
    table: &str,
    id: &str,
    s: &Scope,
) -> CoreResult<Option<T>> {
    conn.block_on(async {
        let row = conn
            .client
            .query_opt(
                &format!(
                    "SELECT record_json FROM {table} WHERE id=$1 AND tenant=$2 \
                      AND (subject IS NULL OR subject=$3) AND (workspace IS NULL OR workspace=$4)"
                ),
                &[&id, &s.tenant, &s.subject, &s.workspace],
            )
            .await
            .map_err(|e| pg_err(e.to_string()))?;
        row.map(|r| serde_json::from_value::<T>(r.get(0)).map_err(|e| pg_err(e.to_string())))
            .transpose()
    })
}

fn delete_scoped(conn: &PgConnection, table: &str, id: &str, s: &Scope) -> CoreResult<bool> {
    conn.block_on(async {
        let n = conn
            .client
            .execute(
                &format!(
                    "DELETE FROM {table} WHERE id=$1 AND tenant=$2 \
                      AND (subject IS NULL OR subject=$3) AND (workspace IS NULL OR workspace=$4)"
                ),
                &[&id, &s.tenant, &s.subject, &s.workspace],
            )
            .await
            .map_err(|e| pg_err(e.to_string()))?;
        Ok(n > 0)
    })
}

fn list_scoped<T: serde::de::DeserializeOwned + Send>(
    conn: &PgConnection,
    table: &str,
    s: &Scope,
) -> CoreResult<Vec<T>> {
    conn.block_on(async {
        let rows = conn
            .client
            .query(
                &format!(
                    "SELECT record_json FROM {table} WHERE tenant=$1 \
                      AND (subject IS NULL OR subject=$2) AND (workspace IS NULL OR workspace=$3)"
                ),
                &[&s.tenant, &s.subject, &s.workspace],
            )
            .await
            .map_err(|e| pg_err(e.to_string()))?;
        rows.iter()
            .map(|r| serde_json::from_value::<T>(r.get(0)).map_err(|e| pg_err(e.to_string())))
            .collect()
    })
}

#[allow(dead_code)]
fn update_jsonb_field(
    conn: &PgConnection,
    table: &str,
    id: &str,
    field_path: &str,
    value: serde_json::Value,
    s: &Scope,
) -> CoreResult<bool> {
    conn.block_on(async {
        let n = conn.client.execute(
            &format!("UPDATE {table} SET record_json = jsonb_set(record_json, '{{\"{field_path}\"}}', $5), last_updated_at=now() \
                      WHERE id=$1 AND tenant=$2 AND (subject IS NULL OR subject=$3) AND (workspace IS NULL OR workspace=$4)"),
            &[&id, &s.tenant, &s.subject, &s.workspace, &value],
        ).await.map_err(|e| pg_err(e.to_string()))?;
        Ok(n > 0)
    })
}

// === PgMemoryService ===

pub struct PgMemoryService {
    conn: PgConnection,
    /// Owning policy authorizer for this service instance. The default is
    /// the host compatibility default (allow-all); `with_dependencies`
    /// injects a stricter deployer authorizer that the owning retrieval path
    /// AND every composed recipe share (r3 construction decision: no
    /// resolver-only permissive bypass, no second policy truth).
    authorizer: std::sync::Arc<dyn engram_runtime::PolicyAuthorizer>,
}

/// Host-level explicit compatibility default shared by the normal retrieval
/// path and every composed evidence recipe (the same role
/// `AllowAllPolicyAuthorizer` plays in the SQLite cell).
pub struct PgAllowAllPolicyAuthorizer;

impl engram_runtime::PolicyAuthorizer for PgAllowAllPolicyAuthorizer {
    fn can_write(
        &self,
        _requester: &engram_domain::Requester,
        _scope: &engram_domain::Scope,
        _policy: &engram_domain::Policy,
    ) -> engram_runtime::CoreResult<()> {
        Ok(())
    }
    fn can_retrieve(
        &self,
        _requester: &engram_domain::Requester,
        _scope: &engram_domain::Scope,
        _policy: &engram_domain::Policy,
    ) -> engram_runtime::CoreResult<()> {
        Ok(())
    }
    fn can_forget(
        &self,
        _requester: &engram_domain::Requester,
        _scope: &engram_domain::Scope,
        _policy: &engram_domain::Policy,
    ) -> engram_runtime::CoreResult<()> {
        Ok(())
    }
}

impl PgMemoryService {
    pub fn new(conn: PgConnection) -> Self {
        Self {
            conn,
            authorizer: std::sync::Arc::new(PgAllowAllPolicyAuthorizer),
        }
    }

    /// Construction with an explicit owning authorizer: the injected
    /// decision is the one the owning retrieval path and every composed
    /// recipe (candidate evidence) share.
    pub fn with_dependencies(
        conn: PgConnection,
        authorizer: std::sync::Arc<dyn engram_runtime::PolicyAuthorizer>,
    ) -> Self {
        Self { conn, authorizer }
    }

    /// The configured owning retrieval authorizer (recipe composition seam).
    pub fn retrieval_authorizer(&self) -> std::sync::Arc<dyn engram_runtime::PolicyAuthorizer> {
        std::sync::Arc::clone(&self.authorizer)
    }
}

#[async_trait]
impl engram_memory::MemoryRepository for PgMemoryService {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        let json = serde_json::to_value(&record).map_err(|e| pg_err(e.to_string()))?;
        put_scoped(
            &self.conn,
            "memories",
            &record.id.to_string(),
            json,
            &record.scope,
        )?;
        Ok(record)
    }
    async fn get_memory(&self, id: &MemoryId, s: &Scope) -> CoreResult<Option<MemoryRecord>> {
        get_scoped(&self.conn, "memories", &id.to_string(), s)
    }
    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent> {
        let json = serde_json::to_value(&event).map_err(|e| pg_err(e.to_string()))?;
        put_scoped(
            &self.conn,
            "memory_events",
            &event.id.to_string(),
            json,
            &event.scope,
        )?;
        Ok(event)
    }
    async fn update_memory_status(
        &self,
        id: &MemoryId,
        s: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        let mut record: MemoryRecord =
            self.get_memory(id, s).await?.ok_or(CoreError::NotFound {
                target_type: "memory",
                target_id: id.to_string(),
            })?;
        record.status = status;
        record.updated_at = Some(chrono::Utc::now());
        let json = serde_json::to_value(&record).map_err(|e| pg_err(e.to_string()))?;
        put_scoped(&self.conn, "memories", &id.to_string(), json, s)?;
        Ok(record)
    }
}

#[async_trait]
impl engram_memory::MemoryEventRepository for PgMemoryService {
    async fn get_event(&self, id: &EventId, s: &Scope) -> CoreResult<Option<MemoryEvent>> {
        get_scoped(&self.conn, "memory_events", &id.to_string(), s)
    }
    async fn list_events_for_memory(
        &self,
        memory_id: &MemoryId,
        s: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        self.conn.block_on(async {
            let rows = self
                .conn
                .client
                .query(
                    "SELECT record_json FROM memory_events WHERE tenant=$1 \
                     AND (subject IS NULL OR subject=$2) AND (workspace IS NULL OR workspace=$3) \
                     AND record_json->>'memoryId' = $4",
                    &[&s.tenant, &s.subject, &s.workspace, &memory_id.to_string()],
                )
                .await
                .map_err(|e| pg_err(e.to_string()))?;
            rows.iter()
                .map(|r| {
                    serde_json::from_value::<MemoryEvent>(r.get(0))
                        .map_err(|e| pg_err(e.to_string()))
                })
                .collect()
        })
    }
    async fn list_events_for_scope(&self, s: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        list_scoped(&self.conn, "memory_events", s)
    }
}

#[async_trait]
impl engram_memory::MemoryService for PgMemoryService {
    async fn write_memory(&self, request: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse> {
        // FIND-code-101: same shared write-rule seam the SQLite path rides —
        // engine-neutral validation in engram-memory, one definition.
        engram_memory::validate_write_request(&request)?;
        let now = chrono::Utc::now();
        let memory_id = MemoryId::from(format!("mem-{}", now.timestamp_nanos_opt().unwrap_or(0)));
        let record = MemoryRecord {
            id: memory_id.clone(),
            kind: request.kind,
            content: request.content,
            scope: request.scope.clone(),
            provenance: request.provenance.clone(),
            policy: request.policy,
            status: MemoryStatus::Active,
            links: request.links,
            assertions: vec![],
            // FIND-code-101: the caller's verdict persists (was hard-coded
            // None, silently discarding every verdict on this backend).
            verdict: request.verdict,
            created_at: now,
            updated_at: None,
            metadata: None,
        };
        let event = MemoryEvent {
            id: EventId::from(format!("evt-{}", now.timestamp_nanos_opt().unwrap_or(0))),
            kind: MemoryEventKind::Written,
            scope: request.scope,
            actor: request.requester.actor,
            memory_id: Some(memory_id),
            payload: request
                .idempotency_key
                .as_ref()
                .map_or_else(|| json!({}), |key| json!({ "idempotencyKey": key })),
            provenance: request.provenance,
            occurred_at: now,
            recorded_at: now,
        };
        self.put_memory(record.clone()).await?;
        self.append_event(event.clone()).await?;
        Ok(WriteMemoryResponse {
            record,
            event,
            deduplicated: None,
        })
    }

    async fn retrieve(&self, request: RetrievalRequest) -> CoreResult<ContextPayload> {
        let records: Vec<MemoryRecord> = list_scoped(&self.conn, "memories", &request.scope)?;
        let items: Vec<RetrievalResult> = records
            .into_iter()
            .filter(|r| r.status == MemoryStatus::Active)
            // Owning policy decision shared with every composed recipe: an
            // unauthorized record is never a retrieval candidate.
            .filter(|r| {
                self.authorizer
                    .can_retrieve(&request.requester, &r.scope, &r.policy)
                    .is_ok()
            })
            .map(|r| RetrievalResult {
                id: format!("result-{}", r.id),
                target_type: RetrievalTargetType::Memory,
                target_id: r.id.to_string(),
                content: r.content.text,
                score: RetrievalScore {
                    total: 1.0,
                    relevance: None,
                    recency: None,
                    confidence: None,
                    cue_match: None,
                    hierarchical_fit: None,
                    policy_fit: Some(1.0),
                },
                provenance: r.provenance,
                policy: r.policy,
                explanation: None,
                fusion_trace: None,
                metadata: None,
            })
            .collect();
        Ok(ContextPayload {
            items,
            budget: None,
            source_failures: vec![],
            omitted: vec![],
            created_at: chrono::Utc::now(),
        })
    }

    async fn forget(&self, request: ForgetRequest) -> CoreResult<ForgetResult> {
        let memory_id = MemoryId::from(request.target_id.clone());
        let deleted = delete_scoped(&self.conn, "memories", &request.target_id, &request.scope)?;
        let event = if deleted {
            let now = chrono::Utc::now();
            let ev = MemoryEvent {
                id: EventId::from(format!("evt-{}", now.timestamp_nanos_opt().unwrap_or(0))),
                kind: match request.mode {
                    DeleteMode::Delete => MemoryEventKind::Forgotten,
                    DeleteMode::Redact => MemoryEventKind::Redacted,
                    DeleteMode::Tombstone => MemoryEventKind::Forgotten,
                    DeleteMode::Archive => MemoryEventKind::Forgotten,
                },
                scope: request.scope,
                actor: request.requester.actor,
                memory_id: Some(memory_id),
                payload: json!({}),
                provenance: Provenance {
                    source: "pgvector-forget".into(),
                    actor: Actor {
                        id: Id::from("system"),
                        kind: ActorKind::System,
                        display_name: None,
                        metadata: None,
                    },
                    observed_at: now,
                    evidence: vec![],
                    derivations: vec![],
                    confidence: None,
                    method: None,
                },
                occurred_at: now,
                recorded_at: now,
            };
            self.append_event(ev.clone()).await?;
            Some(ev)
        } else {
            None
        };
        Ok(ForgetResult {
            target_type: "memory".to_owned(),
            target_id: request.target_id,
            status: if deleted {
                match request.mode {
                    DeleteMode::Delete => ForgetStatus::Deleted,
                    DeleteMode::Tombstone => ForgetStatus::Tombstoned,
                    DeleteMode::Archive => ForgetStatus::Archived,
                    _ => ForgetStatus::Deleted,
                }
            } else {
                ForgetStatus::NotFound
            },
            event,
        })
    }
}

// === PgBeliefStore ===

pub struct PgBeliefStore {
    conn: PgConnection,
}
impl PgBeliefStore {
    pub fn new(conn: PgConnection) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl engram_belief::BeliefRepository for PgBeliefStore {
    async fn put_belief(&self, b: Belief) -> CoreResult<Belief> {
        let json = serde_json::to_value(&b).map_err(|e| pg_err(e.to_string()))?;
        put_scoped(&self.conn, "beliefs", &b.id.to_string(), json, &b.scope)?;
        Ok(b)
    }
    async fn upsert_belief(&self, b: Belief) -> CoreResult<Belief> {
        self.put_belief(b).await
    }
    async fn get_belief(&self, q: engram_belief::BeliefQuery) -> CoreResult<Option<Belief>> {
        let beliefs: Vec<Belief> = list_scoped(&self.conn, "beliefs", &q.scope)?;
        let now = chrono::Utc::now();
        Ok(beliefs.into_iter().find(|b| q.matches_after_scope(b, now)))
    }
    async fn get_belief_by_id(&self, id: &BeliefId, s: &Scope) -> CoreResult<Option<Belief>> {
        get_scoped(&self.conn, "beliefs", &id.to_string(), s)
    }
    async fn mark_stale(&self, id: &BeliefId, s: &Scope, at: Timestamp) -> CoreResult<Belief> {
        let mut b: Belief = self
            .get_belief_by_id(id, s)
            .await?
            .ok_or(CoreError::NotFound {
                target_type: "belief",
                target_id: id.to_string(),
            })?;
        b.status = BeliefStatus::Stale;
        b.stale = Some(true);
        b.updated_at = Some(at);
        self.put_belief(b).await
    }
    async fn clear_stale(&self, id: &BeliefId, s: &Scope, at: Timestamp) -> CoreResult<Belief> {
        let mut b: Belief = self
            .get_belief_by_id(id, s)
            .await?
            .ok_or(CoreError::NotFound {
                target_type: "belief",
                target_id: id.to_string(),
            })?;
        b.status = BeliefStatus::Active;
        b.stale = None;
        b.updated_at = Some(at);
        self.put_belief(b).await
    }
    async fn supersede_belief(
        &self,
        id: &BeliefId,
        s: &Scope,
        replacement: BeliefId,
        at: Timestamp,
    ) -> CoreResult<Belief> {
        let mut b: Belief = self
            .get_belief_by_id(id, s)
            .await?
            .ok_or(CoreError::NotFound {
                target_type: "belief",
                target_id: id.to_string(),
            })?;
        b.status = BeliefStatus::Superseded;
        b.superseded_by = Some(replacement);
        b.updated_at = Some(at);
        self.put_belief(b).await
    }
    async fn retract_belief(&self, id: &BeliefId, s: &Scope, at: Timestamp) -> CoreResult<Belief> {
        let mut b: Belief = self
            .get_belief_by_id(id, s)
            .await?
            .ok_or(CoreError::NotFound {
                target_type: "belief",
                target_id: id.to_string(),
            })?;
        b.status = BeliefStatus::Retracted;
        b.valid_until = Some(at);
        b.updated_at = Some(at);
        self.put_belief(b).await
    }
    async fn list_stale(&self, s: &Scope) -> CoreResult<Vec<Belief>> {
        let all: Vec<Belief> = list_scoped(&self.conn, "beliefs", s)?;
        Ok(all
            .into_iter()
            .filter(|b| b.status == BeliefStatus::Stale || b.stale == Some(true))
            .collect())
    }
    async fn list_contradictions(&self, s: &Scope) -> CoreResult<Vec<Contradiction>> {
        list_scoped(&self.conn, "contradictions", s)
    }
    async fn beliefs_referencing_source(
        &self,
        q: engram_belief::BeliefReferenceQuery,
    ) -> CoreResult<Vec<Belief>> {
        let all: Vec<Belief> = list_scoped(&self.conn, "beliefs", &q.scope)?;
        let at = q.valid_at.unwrap_or_else(chrono::Utc::now);
        Ok(all
            .into_iter()
            .filter(|b| {
                engram_belief::belief_references_source(b, &q.source_type, &q.source_id, at)
            })
            .collect())
    }
    async fn put_contradiction(&self, c: Contradiction) -> CoreResult<Contradiction> {
        let json = serde_json::to_value(&c).map_err(|e| pg_err(e.to_string()))?;
        put_scoped(
            &self.conn,
            "contradictions",
            &c.id.to_string(),
            json,
            &c.scope,
        )?;
        Ok(c)
    }
    async fn get_contradiction(
        &self,
        id: &ContradictionId,
        s: &Scope,
    ) -> CoreResult<Option<Contradiction>> {
        get_scoped(&self.conn, "contradictions", &id.to_string(), s)
    }
    async fn resolve_contradiction(
        &self,
        id: &ContradictionId,
        s: &Scope,
        res: ContradictionResolution,
    ) -> CoreResult<Contradiction> {
        let mut c: Contradiction =
            self.get_contradiction(id, s)
                .await?
                .ok_or(CoreError::NotFound {
                    target_type: "contradiction",
                    target_id: id.to_string(),
                })?;
        c.status = match res.kind {
            ContradictionResolutionKind::ManualIgnore => ContradictionStatus::Ignored,
            ContradictionResolutionKind::NeedsMoreEvidence => ContradictionStatus::Open,
            _ => ContradictionStatus::Resolved,
        };
        c.updated_at = Some(res.resolved_at);
        c.resolution = Some(res);
        let json = serde_json::to_value(&c).map_err(|e| pg_err(e.to_string()))?;
        put_scoped(&self.conn, "contradictions", &id.to_string(), json, s)?;
        Ok(c)
    }
}

// === PgHierarchyStore ===

pub struct PgHierarchyStore {
    conn: PgConnection,
}
impl PgHierarchyStore {
    pub fn new(conn: PgConnection) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl engram_hierarchy::HierarchyRepository for PgHierarchyStore {
    async fn put_node(&self, n: HierarchyNode) -> CoreResult<HierarchyNode> {
        let json = serde_json::to_value(&n).map_err(|e| pg_err(e.to_string()))?;
        put_scoped(
            &self.conn,
            "hierarchy_nodes",
            &n.id.to_string(),
            json,
            &n.scope,
        )?;
        Ok(n)
    }
    async fn put_relation(&self, r: HierarchyRelation) -> CoreResult<HierarchyRelation> {
        let json = serde_json::to_value(&r).map_err(|e| pg_err(e.to_string()))?;
        put_scoped(
            &self.conn,
            "hierarchy_relations",
            &r.id.to_string(),
            json,
            &r.scope,
        )?;
        Ok(r)
    }
    async fn path_for(
        &self,
        seed_ids: &[String],
        _s: &Scope,
        max_layer: Option<u32>,
    ) -> CoreResult<HierarchyPath> {
        Ok(HierarchyPath {
            seed_ids: seed_ids.to_vec(),
            lca_id: None,
            nodes: vec![],
            relations: vec![],
            max_layer,
        })
    }
}

// === PgProcedureStore ===

pub struct PgProcedureStore {
    conn: PgConnection,
}
impl PgProcedureStore {
    pub fn new(conn: PgConnection) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl engram_procedures::ProcedureRepository for PgProcedureStore {
    async fn upsert_procedure(&self, p: Procedure) -> CoreResult<Procedure> {
        let json = serde_json::to_value(&p).map_err(|e| pg_err(e.to_string()))?;
        put_scoped(&self.conn, "procedures", &p.id.to_string(), json, &p.scope)?;
        Ok(p)
    }
    async fn get_procedure(&self, id: &ProcedureId, s: &Scope) -> CoreResult<Option<Procedure>> {
        get_scoped(&self.conn, "procedures", &id.to_string(), s)
    }
    async fn get_procedure_by_name(&self, name: &str, s: &Scope) -> CoreResult<Option<Procedure>> {
        let all: Vec<Procedure> = list_scoped(&self.conn, "procedures", s)?;
        Ok(all.into_iter().find(|p| p.name == name))
    }
    async fn list_procedures(&self, s: &Scope) -> CoreResult<Vec<Procedure>> {
        list_scoped(&self.conn, "procedures", s)
    }
    async fn increment_success(&self, id: &ProcedureId, s: &Scope) -> CoreResult<Procedure> {
        let mut p: Procedure = self
            .get_procedure(id, s)
            .await?
            .ok_or(CoreError::NotFound {
                target_type: "procedure",
                target_id: id.to_string(),
            })?;
        p.success_count += 1;
        self.upsert_procedure(p).await
    }
    async fn increment_failure(&self, id: &ProcedureId, s: &Scope) -> CoreResult<Procedure> {
        let mut p: Procedure = self
            .get_procedure(id, s)
            .await?
            .ok_or(CoreError::NotFound {
                target_type: "procedure",
                target_id: id.to_string(),
            })?;
        p.failure_count += 1;
        self.upsert_procedure(p).await
    }
    async fn procedure_stats(&self, s: &Scope) -> CoreResult<ProcedureStats> {
        let all: Vec<Procedure> = list_scoped(&self.conn, "procedures", s)?;
        Ok(ProcedureStats {
            total: all.len(),
            total_success: all.iter().map(|p| p.success_count as u64).sum(),
            total_failure: all.iter().map(|p| p.failure_count as u64).sum(),
        })
    }
    async fn delete_procedure(&self, id: &ProcedureId, s: &Scope) -> CoreResult<bool> {
        delete_scoped(&self.conn, "procedures", &id.to_string(), s)
    }
}

// === Scope inventory (job 012-admin-console-v1, STEP-003) ===
//
// The port trait lives in the facade (`engram_integration::ScopeInventory`);
// this crate implements core ports only, so — exactly like the SQLite mirror —
// the capability is implemented here as inherent methods and the trait impl +
// recipe wiring live in `backends/pgvector` (the composition layer).
//
// Semantics are the unified partition semantics, NOT the divergent
// `list_scoped` predicates above: NULL dimensions are their own distinct value
// (Postgres `GROUP BY` groups NULLs together), and partition reads match
// every dimension with `IS NOT DISTINCT FROM` — never the
// `(subject IS NULL OR subject=$n)` wildcard shapes used by the scoped reads.

/// One grouped row of the scope inventory: a distinct partition tuple plus
/// the number of governed rows of one kind stored under exactly that tuple.
/// Shape-mirrors the SQLite adapter's row type (the two adapter crates cannot
/// share a type); the port's merge treats them identically.
#[derive(Debug, Clone, PartialEq)]
pub struct PgScopeCountRow {
    pub scope: Scope,
    pub count: u64,
}

/// pgvector scope-inventory reader over the governed Postgres tables.
///
/// One connection serves every table (all governed tables share the
/// database); every read is a short read-only query.
pub struct PgScopeInventory {
    conn: PgConnection,
}

/// The exact-tuple partition predicate: every scope dimension matched with
/// NULL-equality (`IS NOT DISTINCT FROM`), the unified partition semantics.
const EXACT_PARTITION_WHERE: &str = "tenant=$1 \
     AND subject IS NOT DISTINCT FROM $2 \
     AND workspace IS NOT DISTINCT FROM $3 \
     AND session IS NOT DISTINCT FROM $4 \
     AND environment IS NOT DISTINCT FROM $5";

impl PgScopeInventory {
    pub fn new(conn: PgConnection) -> Self {
        Self { conn }
    }

    /// Distinct memory partitions with row counts (NULL dimensions are their
    /// own group; counts reflect rows present — the pgvector tables carry no
    /// archive column).
    pub fn count_memory_partitions(&self) -> CoreResult<Vec<PgScopeCountRow>> {
        self.count_partitions("memories")
    }

    /// Distinct knowledge-entity partitions with row counts.
    pub fn count_entity_partitions(&self) -> CoreResult<Vec<PgScopeCountRow>> {
        self.count_partitions("knowledge_entities")
    }

    /// Distinct belief partitions with row counts.
    pub fn count_belief_partitions(&self) -> CoreResult<Vec<PgScopeCountRow>> {
        self.count_partitions("beliefs")
    }

    fn count_partitions(&self, table: &str) -> CoreResult<Vec<PgScopeCountRow>> {
        // Table names come from this crate's own call sites only.
        let table = match table {
            "memories" | "knowledge_entities" | "knowledge_relationships" | "beliefs" => table,
            other => {
                return Err(pg_err(format!(
                    "scope inventory queried an unexpected table: {other}"
                )))
            }
        };
        self.conn
            .block_on(async {
                let rows = self
                    .conn
                    .client
                    .query(
                        &format!(
                            "SELECT tenant, subject, workspace, session, environment, COUNT(*) \
                             FROM {table} \
                             GROUP BY tenant, subject, workspace, session, environment"
                        ),
                        &[],
                    )
                    .await
                    .map_err(|e| pg_err(e.to_string()))?;
                Ok(rows
                    .iter()
                    .map(|row| PgScopeCountRow {
                        scope: Scope {
                            tenant: row.get(0),
                            subject: row.get(1),
                            workspace: row.get(2),
                            session: row.get(3),
                            environment: row.get(4),
                        },
                        count: row.get::<_, i64>(5) as u64,
                    })
                    .collect())
            })
            .map_err(|e: CoreError| e)
    }

    /// Memories of exactly one partition, ascending by record id, bounded by
    /// `limit`/`offset`.
    pub fn list_partition_memories(
        &self,
        partition: &Scope,
        limit: i64,
        offset: i64,
    ) -> CoreResult<Vec<MemoryRecord>> {
        self.list_partition_records(
            "memories",
            partition,
            limit,
            offset,
            "ORDER BY id ASC LIMIT $6 OFFSET $7",
        )
    }

    /// Knowledge entities of exactly one partition, ascending by record id,
    /// bounded by `limit`/`offset`.
    pub fn list_partition_entities(
        &self,
        partition: &Scope,
        limit: i64,
        offset: i64,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        self.list_partition_records(
            "knowledge_entities",
            partition,
            limit,
            offset,
            "ORDER BY id ASC LIMIT $6 OFFSET $7",
        )
    }

    /// Knowledge relationships of exactly one partition, ascending by record
    /// id, bounded by `limit`/`offset`.
    pub fn list_partition_relationships(
        &self,
        partition: &Scope,
        limit: i64,
        offset: i64,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.list_partition_records(
            "knowledge_relationships",
            partition,
            limit,
            offset,
            "ORDER BY id ASC LIMIT $6 OFFSET $7",
        )
    }

    /// The most recently created memories of exactly one partition, newest
    /// first (`created_at DESC, id DESC`), bounded by `limit`. The shared
    /// listing binds `OFFSET $7` as 0 (Postgres requires every bound
    /// parameter to appear in the statement).
    pub fn list_recent_partition_memories(
        &self,
        partition: &Scope,
        limit: i64,
    ) -> CoreResult<Vec<MemoryRecord>> {
        self.list_partition_records(
            "memories",
            partition,
            limit,
            0,
            "ORDER BY created_at DESC, id DESC LIMIT $6 OFFSET $7",
        )
    }

    /// Shared exact-partition listing over one governed table.
    fn list_partition_records<T: serde::de::DeserializeOwned + Send>(
        &self,
        table: &str,
        partition: &Scope,
        limit: i64,
        offset: i64,
        tail: &str,
    ) -> CoreResult<Vec<T>> {
        let tenant = partition.tenant.clone();
        let subject = partition.subject.clone();
        let workspace = partition.workspace.clone();
        let session = partition.session.clone();
        let environment = partition.environment.clone();
        self.conn.block_on(async {
            let rows = self
                .conn
                .client
                .query(
                    &format!(
                        "SELECT record_json FROM {table} WHERE {EXACT_PARTITION_WHERE} {tail}"
                    ),
                    &[
                        &tenant,
                        &subject,
                        &workspace,
                        &session,
                        &environment,
                        &limit,
                        &offset,
                    ],
                )
                .await
                .map_err(|e| pg_err(e.to_string()))?;
            rows.iter()
                .map(|row| {
                    serde_json::from_value::<T>(row.get(0)).map_err(|e| pg_err(e.to_string()))
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod scope_inventory_tests {
    //! STEP-003 focused proofs: the pgvector scope-inventory aggregation and
    //! partition-scoped listings against a REAL marked-disposable Postgres,
    //! exercised through the cells' own write APIs. Gated behind `--ignored` +
    //! `ENGRAM_TEST_PG_URL` like every accepted PG suite.

    use super::*;
    use engram_domain::{
        Actor, ActorKind, AllowedUse, Belief, BeliefId, BeliefStatus, BeliefSubject, EntityKind,
        EntityRef, Id, KnowledgeEntity, MemoryContent, MemoryKind, Policy, Provenance, Requester,
        Retention, Visibility, WriteMemoryRequest,
    };
    use futures::executor::block_on;

    fn url() -> String {
        std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required")
    }

    /// Asserts the disposable marker before touching the fixture (the accepted
    /// PG-suite contract; CI marks its ephemeral service the same way).
    fn assert_disposable(conn: &PgConnection) {
        let marker: Option<String> = conn
            .block_on(async {
                conn.client
                    .query_one(
                        "SELECT shobj_description(oid,'pg_database') FROM pg_database \
                         WHERE datname=current_database()",
                        &[],
                    )
                    .await
            })
            .expect("marker query")
            .get(0);
        assert!(
            marker
                .as_deref()
                .is_some_and(|m| m.contains("disposable=true")),
            "refusing to run against a database not marked disposable"
        );
    }

    struct Fixture {
        tenant: String,
        memory: PgMemoryService,
        knowledge: crate::knowledge::PgKnowledgeStore,
        belief: PgBeliefStore,
    }

    fn fixture() -> Fixture {
        let conn = PgConnection::connect(&url()).expect("connect fixture");
        assert_disposable(&conn);
        conn.block_on(async {
            conn.client
                .batch_execute(&crate::schema::schema_sql_without_vectors())
                .await
        })
        .expect("apply schema");
        let tenant = format!(
            "job012-inv-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        Fixture {
            memory: PgMemoryService::new(PgConnection::connect(&url()).expect("memory conn")),
            knowledge: crate::knowledge::PgKnowledgeStore::new(
                PgConnection::connect(&url()).expect("knowledge conn"),
            ),
            belief: PgBeliefStore::new(PgConnection::connect(&url()).expect("belief conn")),
            tenant,
        }
    }

    fn inventory(fixture: &Fixture) -> PgScopeInventory {
        PgScopeInventory::new(PgConnection::connect(&url()).expect("inventory conn"))
    }

    fn scope(fixture: &Fixture, workspace: Option<&str>) -> Scope {
        Scope {
            tenant: fixture.tenant.clone(),
            subject: None,
            workspace: workspace.map(str::to_string),
            session: None,
            environment: None,
        }
    }

    fn actor() -> Actor {
        Actor {
            id: Id::from("job012-scope-inventory"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        }
    }

    fn write_req(text: &str, scope: Scope) -> WriteMemoryRequest {
        let now = chrono::Utc::now();
        WriteMemoryRequest {
            kind: MemoryKind::Observation,
            content: MemoryContent {
                text: text.to_owned(),
                summary: None,
                entities: Vec::new(),
                language: None,
                format: None,
                structured: None,
                hash: None,
            },
            scope,
            requester: Requester {
                actor: actor(),
                roles: Vec::new(),
                permissions: vec!["memory.write".to_owned()],
                on_behalf_of: None,
            },
            provenance: Provenance {
                source: "job012-scope-inventory".to_owned(),
                actor: actor(),
                observed_at: now,
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: None,
            },
            policy: Policy {
                visibility: Visibility::Private,
                retention: Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            links: Vec::new(),
            verdict: None,
            idempotency_key: None,
        }
    }

    fn entity(id: &str, name: &str, scope: Scope) -> KnowledgeEntity {
        KnowledgeEntity {
            id: Id::new(id).expect("entity id"),
            graph_id: None,
            kind: EntityKind::Project,
            name: name.to_owned(),
            aliases: Vec::new(),
            scope,
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: Provenance {
                source: "job012-scope-inventory".to_owned(),
                actor: actor(),
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: None,
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            valid_from: None,
            valid_until: None,
            archived_at: None,
            metadata: None,
        }
    }

    fn belief(id: &str, scope: Scope) -> Belief {
        Belief {
            id: BeliefId::new(id).expect("belief id"),
            scope,
            subject: BeliefSubject {
                key: format!("belief-{id}"),
                entity_ref: None,
                concept_ref: None,
                aliases: Vec::new(),
            },
            content: format!("belief {id}"),
            status: BeliefStatus::Active,
            confidence: 0.9,
            sources: Vec::new(),
            valid_from: None,
            valid_until: None,
            superseded_by: None,
            stale: None,
            synthesizer: None,
            reasoning: None,
            embedding_refs: Vec::new(),
            policy: Policy {
                visibility: Visibility::Private,
                retention: Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            provenance: Provenance {
                source: "job012-scope-inventory".to_owned(),
                actor: actor(),
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: None,
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        }
    }

    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL against the approved marked disposable fixture"]
    fn scope_inventory_groups_null_as_its_own_value() {
        let fx = fixture();
        let inv = inventory(&fx);
        let unset = scope(&fx, None);
        let defaulted = scope(&fx, Some("default"));
        block_on(async {
            use engram_memory::MemoryService;
            fx.memory
                .write_memory(write_req("unset workspace memory", unset))
                .await
                .expect("write");
            fx.memory
                .write_memory(write_req("default workspace memory", defaulted))
                .await
                .expect("write");
        });
        let rows = inv.count_memory_partitions().expect("memory partitions");
        let mine: Vec<_> = rows
            .iter()
            .filter(|r| r.scope.tenant == fx.tenant)
            .collect();
        assert_eq!(
            mine.len(),
            2,
            "NULL workspace and 'default' are distinct partitions"
        );
        assert_eq!(
            mine.iter()
                .find(|r| r.scope.workspace.is_none())
                .expect("unset row")
                .count,
            1
        );
        assert_eq!(
            mine.iter()
                .find(|r| r.scope.workspace.as_deref() == Some("default"))
                .expect("default row")
                .count,
            1
        );
    }

    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL against the approved marked disposable fixture"]
    fn scope_inventory_counts_three_kinds_and_lists_exactly() {
        let fx = fixture();
        let inv = inventory(&fx);
        let a = scope(&fx, Some("proj-a"));
        let sibling = scope(&fx, Some("proj-b"));
        let unset = scope(&fx, None);
        block_on(async {
            use engram_belief::BeliefRepository;
            use engram_knowledge::KnowledgeRepository;
            use engram_memory::MemoryService;
            fx.memory
                .write_memory(write_req("a1", a.clone()))
                .await
                .expect("write");
            fx.memory
                .write_memory(write_req("a2", a.clone()))
                .await
                .expect("write");
            fx.memory
                .write_memory(write_req("sibling-only", sibling.clone()))
                .await
                .expect("write");
            fx.memory
                .write_memory(write_req("unset-only", unset.clone()))
                .await
                .expect("write");
            // Run-unique ids: the fixture database persists across runs and
            // `put_entity`'s conflict path keeps the stored scope columns, so
            // reused ids would silently strand rows under a prior run's scope.
            for i in 1..=3 {
                fx.knowledge
                    .put_entity(entity(
                        &format!("{}-ent-a-{i}", fx.tenant),
                        &format!("Entity {i}"),
                        a.clone(),
                    ))
                    .await
                    .expect("put entity");
            }
            fx.belief
                .put_belief(belief(&format!("{}-bel-a-1", fx.tenant), a.clone()))
                .await
                .expect("put belief");
        });

        let memories = inv
            .count_memory_partitions()
            .expect("memories")
            .into_iter()
            .find(|r| r.scope == a)
            .expect("partition A memory row");
        assert_eq!(memories.count, 2);
        let entities = inv
            .count_entity_partitions()
            .expect("entities")
            .into_iter()
            .find(|r| r.scope == a)
            .expect("partition A entity row");
        assert_eq!(entities.count, 3);
        let beliefs = inv
            .count_belief_partitions()
            .expect("beliefs")
            .into_iter()
            .find(|r| r.scope == a)
            .expect("partition A belief row");
        assert_eq!(beliefs.count, 1);

        let listed = inv
            .list_partition_memories(&a, 10, 0)
            .expect("list memories");
        let texts: Vec<&str> = listed.iter().map(|m| m.content.text.as_str()).collect();
        assert_eq!(texts.len(), 2);
        assert!(texts.contains(&"a1") && texts.contains(&"a2"));
        assert!(
            !texts.contains(&"sibling-only") && !texts.contains(&"unset-only"),
            "exact-tuple predicate excludes sibling and '(unset)' partitions"
        );
        let mut ids: Vec<String> = listed.iter().map(|m| m.id.to_string()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        ids.sort();
        assert_eq!(ids, sorted, "ascending id order");

        let entities_listed = inv
            .list_partition_entities(&a, 10, 0)
            .expect("list entities");
        assert_eq!(entities_listed.len(), 3);

        let unset_listed = inv
            .list_partition_memories(&unset, 10, 0)
            .expect("list unset");
        assert_eq!(
            unset_listed
                .iter()
                .map(|m| m.content.text.clone())
                .collect::<Vec<_>>(),
            vec!["unset-only".to_owned()],
            "the '(unset)' partition is addressable exactly"
        );
    }

    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL against the approved marked disposable fixture"]
    fn scope_inventory_recent_memories_newest_first() {
        let fx = fixture();
        let inv = inventory(&fx);
        let a = scope(&fx, Some("recent"));
        let other = scope(&fx, Some("other-recent"));
        block_on(async {
            use engram_memory::MemoryService;
            fx.memory
                .write_memory(write_req("r1", a.clone()))
                .await
                .expect("write");
            fx.memory
                .write_memory(write_req("r2", a.clone()))
                .await
                .expect("write");
            fx.memory
                .write_memory(write_req("r3", a.clone()))
                .await
                .expect("write");
            fx.memory
                .write_memory(write_req("elsewhere", other))
                .await
                .expect("write");
        });
        let recent = inv.list_recent_partition_memories(&a, 2).expect("recent");
        assert_eq!(recent.len(), 2, "bounded by limit");
        let texts: Vec<&str> = recent.iter().map(|m| m.content.text.as_str()).collect();
        assert_eq!(texts, vec!["r3", "r2"], "newest first, partition-confined");
    }
}
