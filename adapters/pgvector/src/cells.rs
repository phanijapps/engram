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
}

impl PgMemoryService {
    pub fn new(conn: PgConnection) -> Self {
        Self { conn }
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
        Ok(beliefs
            .into_iter()
            .filter(|b| q.matches_after_scope(b, now))
            .next())
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
