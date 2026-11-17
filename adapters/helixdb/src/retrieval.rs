//! Engine candidate access only; authorizers/fusion/budgeting stay canonical.
use crate::{HelixStore, error::Error, records::*, transport::Operation};
use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult, PolicyAuthorizer};
use helix_db::dsl::prelude::*;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
pub(crate) const TEXT_FAMILIES: &[&str] = &[
    MEMORY,
    CHUNK,
    ENTITY,
    RELATIONSHIP,
    CONCEPT,
    BELIEF,
    HIERARCHY_NODE,
];
pub(crate) fn text_projection(family: &str, v: &Value) -> Option<String> {
    if !TEXT_FAMILIES.contains(&family) {
        return None;
    }
    if family == CONCEPT {
        let concept: Concept = serde_json::from_value(v.clone()).ok()?;
        return Some(
            std::iter::once(concept.pref_label.value)
                .chain(concept.alt_labels.into_iter().map(|l| l.value))
                .chain(concept.definition)
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    let mut text = vec![];
    for field in [
        "name",
        "summary",
        "description",
        "predicate",
        "text",
        "content",
    ] {
        if let Some(s) = v.get(field).and_then(Value::as_str) {
            text.push(s.to_owned())
        }
    }
    if let Some(s) = v.pointer("/content/text").and_then(Value::as_str) {
        text.push(s.to_owned())
    }
    Some(text.join(" "))
}
#[derive(Clone)]
pub struct HelixRetrievalIndex {
    store: HelixStore,
    authorizer: Arc<dyn PolicyAuthorizer>,
    query_vector: Option<Arc<dyn QueryVectorProvider>>,
}
impl HelixStore {
    pub fn retrieval_index(&self, authorizer: Arc<dyn PolicyAuthorizer>) -> HelixRetrievalIndex {
        HelixRetrievalIndex {
            store: self.clone(),
            authorizer,
            query_vector: None,
        }
    }
}
fn target(family: &str) -> RetrievalTargetType {
    match family {
        MEMORY => RetrievalTargetType::Memory,
        CHUNK => RetrievalTargetType::Chunk,
        ENTITY => RetrievalTargetType::Entity,
        RELATIONSHIP => RetrievalTargetType::Relationship,
        CONCEPT => RetrievalTargetType::Concept,
        BELIEF => RetrievalTargetType::Belief,
        _ => RetrievalTargetType::HierarchyNode,
    }
}
/// Recipe-owned query embeddings; this adapter never resolves or initializes a model.
pub trait QueryVectorProvider: Send + Sync {
    fn query_vector(&self, request: &RetrievalRequest) -> CoreResult<(EmbeddingSpace, Vec<f32>)>;
}
#[derive(Clone)]
struct Candidate {
    raw: Value,
    policy: Policy,
    provenance: Provenance,
    scope: Scope,
    created_at: Timestamp,
    confidence: Option<f32>,
    concepts: Vec<ConceptId>,
    memory_kind: Option<MemoryKind>,
    chunk_kind: Option<KnowledgeChunkKind>,
    source_kind: Option<SourceKind>,
    entity: Option<KnowledgeEntity>,
    belief: Option<Belief>,
    archived: bool,
}
impl Candidate {
    fn new(
        raw: &Value,
        policy: Policy,
        provenance: Provenance,
        scope: Scope,
        created_at: Timestamp,
    ) -> Self {
        let confidence = provenance.confidence;
        Self {
            raw: raw.clone(),
            policy,
            provenance,
            scope,
            created_at,
            confidence,
            concepts: vec![],
            memory_kind: None,
            chunk_kind: None,
            source_kind: None,
            entity: None,
            belief: None,
            archived: false,
        }
    }
}
impl HelixRetrievalIndex {
    pub fn with_query_vector_provider(mut self, provider: Arc<dyn QueryVectorProvider>) -> Self {
        self.query_vector = Some(provider);
        self
    }
    fn result(
        &self,
        family: &str,
        id: &str,
        v: &Candidate,
        score: f32,
    ) -> CoreResult<RetrievalResult> {
        Ok(RetrievalResult {
            id: format!("helix:{family}:{id}"),
            target_type: target(family),
            target_id: id.into(),
            content: text_projection(family, &v.raw).unwrap_or_default(),
            score: RetrievalScore {
                total: score,
                relevance: Some(score),
                recency: None,
                confidence: v.confidence,
                cue_match: None,
                hierarchical_fit: None,
                policy_fit: Some(1.0),
            },
            provenance: v.provenance.clone(),
            policy: v.policy.clone(),
            explanation: None,
            fusion_trace: None,
            metadata: v
                .raw
                .get("metadata")
                .cloned()
                .map(decode)
                .transpose()
                .map_err(core)?,
        })
    }
    fn graph_policy(&self, graph: Option<&KnowledgeGraphId>) -> CoreResult<Option<Policy>> {
        match graph {
            None => Ok(Some(Policy::graph_lane_default())),
            Some(id) => Ok(self
                .store
                .get::<KnowledgeGraph>(GRAPH, id.as_str())
                .map_err(core)?
                .filter(|g| self.store.visible_scope(&g.scope))
                .map(|g| g.policy)),
        }
    }
    /// Decode unchanged canonical shapes and resolve their storage-owned ancestry once.
    fn project(&self, family: &str, raw: &Value) -> CoreResult<Option<Candidate>> {
        let launch_scope = self.store.inner.partition.scope.clone();
        let mut c = match family {
            MEMORY => {
                let v: MemoryRecord = decode(raw.clone()).map_err(core)?;
                let archived = v.status == MemoryStatus::Archived;
                let mut c = Candidate::new(raw, v.policy, v.provenance, v.scope, v.created_at);
                c.memory_kind = Some(v.kind);
                c.archived = archived;
                c
            }
            CHUNK => {
                let v: KnowledgeChunk = decode(raw.clone()).map_err(core)?;
                let Some(source) = self
                    .store
                    .get::<KnowledgeSource>(SOURCE, v.source_id.as_str())
                    .map_err(core)?
                else {
                    return Ok(None);
                };
                let Some(document) = self
                    .store
                    .get::<SourceDocument>(DOCUMENT, v.document_id.as_str())
                    .map_err(core)?
                else {
                    return Ok(None);
                };
                if source.scope != launch_scope || document.source_id != v.source_id {
                    return Ok(None);
                }
                let mut c = Candidate::new(raw, v.policy, v.provenance, source.scope, v.created_at);
                c.concepts = v.concepts.into_iter().filter_map(|r| r.id).collect();
                c.chunk_kind = Some(v.kind);
                c.source_kind = Some(source.kind);
                c
            }
            ENTITY => {
                let v: KnowledgeEntity = decode(raw.clone()).map_err(core)?;
                let Some(policy) = self.graph_policy(v.graph_id.as_ref())? else {
                    return Ok(None);
                };
                let mut c = Candidate::new(
                    raw,
                    policy,
                    v.provenance.clone(),
                    v.scope.clone(),
                    v.created_at,
                );
                c.concepts = v.concept_refs.iter().filter_map(|r| r.id.clone()).collect();
                c.archived = v.archived_at.is_some();
                c.entity = Some(v);
                c
            }
            RELATIONSHIP => {
                let v: KnowledgeRelationship = decode(raw.clone()).map_err(core)?;
                let Some(policy) = self.graph_policy(v.graph_id.as_ref())? else {
                    return Ok(None);
                };
                let confidence = v.confidence.or(v.provenance.confidence);
                let archived = v.archived_at.is_some();
                let mut c = Candidate::new(raw, policy, v.provenance, v.scope, v.created_at);
                c.confidence = confidence;
                c.archived = archived;
                c
            }
            CONCEPT => {
                let v: Concept = decode(raw.clone()).map_err(core)?;
                let Some(scheme) = self
                    .store
                    .get::<ConceptScheme>(SCHEME, v.scheme_id.as_str())
                    .map_err(core)?
                else {
                    return Ok(None);
                };
                if scheme.scope != launch_scope {
                    return Ok(None);
                }
                let mut c =
                    Candidate::new(raw, scheme.policy, v.provenance, scheme.scope, v.created_at);
                c.concepts.push(v.id);
                c.archived = matches!(
                    v.status,
                    ConceptStatus::Deprecated | ConceptStatus::Merged | ConceptStatus::Rejected
                );
                c
            }
            BELIEF => {
                let v: Belief = decode(raw.clone()).map_err(core)?;
                let mut c = Candidate::new(
                    raw,
                    v.policy.clone(),
                    v.provenance.clone(),
                    v.scope.clone(),
                    v.created_at,
                );
                c.confidence = Some(v.confidence);
                c.belief = Some(v);
                c
            }
            HIERARCHY_NODE => {
                let v: HierarchyNode = decode(raw.clone()).map_err(core)?;
                let archived = v.status != HierarchyNodeStatus::Active;
                let mut c = Candidate::new(raw, v.policy, v.provenance, v.scope, v.created_at);
                c.archived = archived;
                c
            }
            _ => return Err(core(Error::IncompatibleSchema)),
        };
        if c.scope != launch_scope {
            return Ok(None);
        }
        c.archived |= raw.get("archivedAt").is_some_and(|v| !v.is_null());
        Ok(Some(c))
    }
    fn semantic(&self, q: &RetrievalRequest, limit: usize) -> CoreResult<Vec<RetrievalResult>> {
        let provider = self
            .query_vector
            .as_ref()
            .ok_or_else(|| CoreError::InvalidRequest {
                reason: "no query-vector collaborator configured".into(),
            })?;
        // Disabled-vector construction exits before calling any query/model collaborator.
        let index = self.store.vector_index()?;
        let mut allowed = BTreeMap::new();
        for family in TEXT_FAMILIES {
            for v in self.store.all::<Value>(family).map_err(core)? {
                if let Some(candidate) = self.project(family, &v)? {
                    if !self.eligible(&candidate, q)? {
                        continue;
                    }
                    let id = id(&v).map_err(core)?.to_owned();
                    if allowed.insert(id, (*family, candidate)).is_some() {
                        return Err(CoreError::InvalidRequest {
                            reason: "ambiguous retrieval target identity across families".into(),
                        });
                    }
                }
            }
        }
        let (space, vector) = provider.query_vector(q)?;
        let ids = allowed.keys().cloned().collect::<Vec<_>>();
        let hits = index.search_constrained(&space, vector, limit, Some(&ids))?;
        hits.into_iter()
            .map(|(id, score)| {
                let (family, v) = allowed
                    .get(id.as_str())
                    .ok_or_else(|| core(Error::IncompatibleSchema))?;
                self.result(family, id.as_str(), v, score)
            })
            .collect()
    }
    fn eligible(&self, c: &Candidate, q: &RetrievalRequest) -> CoreResult<bool> {
        if c.scope != q.scope
            || c.policy
                .expires_at
                .is_some_and(|at| at <= chrono::Utc::now())
            || self
                .authorizer
                .can_retrieve(&q.requester, &c.scope, &c.policy)
                .is_err()
        {
            return Ok(false);
        }
        if c.archived
            && !q
                .filters
                .as_ref()
                .is_some_and(|f| f.include_archived == Some(true))
        {
            return Ok(false);
        }
        if matches!(
            c.raw.get("status").and_then(Value::as_str),
            Some("forgotten" | "redacted" | "superseded" | "retracted" | "stale")
        ) {
            return Ok(false);
        }
        if let Some(b) = &c.belief {
            if !engram_belief::is_live_belief(b)
                || !engram_belief::live_at(
                    b.valid_from,
                    b.valid_until,
                    q.filters
                        .as_ref()
                        .and_then(|f| f.as_of)
                        .unwrap_or_else(chrono::Utc::now),
                )
            {
                return Ok(false);
            }
        }
        if c.raw.pointer("/metadata/repositoryId").is_some()
            || c.raw.pointer("/metadata/generationId").is_some()
            || matches!(
                c.chunk_kind,
                Some(
                    KnowledgeChunkKind::CodeSymbol
                        | KnowledgeChunkKind::CodeBlock
                        | KnowledgeChunkKind::DiffHunk
                )
            )
        {
            return Ok(false);
        }
        if let Some(e) = &c.entity {
            if engram_retrieval::suppresses_code_artifacts(q)
                && engram_retrieval::is_code_artifact(e)
            {
                return Ok(false);
            }
            if let Some(at) = q.filters.as_ref().and_then(|f| f.as_of) {
                if !engram_belief::interval_contains(e.valid_from, e.valid_until, at) {
                    return Ok(false);
                }
            }
        }
        if let Some(f) = &q.filters {
            if f.min_confidence
                .is_some_and(|n| c.confidence.unwrap_or(0.0) < n)
                || f.since.is_some_and(|at| c.created_at < at)
                || f.until.is_some_and(|at| c.created_at > at)
            {
                return Ok(false);
            }
            if let Some(kind) = &c.memory_kind {
                if !f.memory_kinds.is_empty() && !f.memory_kinds.contains(kind) {
                    return Ok(false);
                }
            }
            if let Some(kind) = &c.chunk_kind {
                if !f.chunk_kinds.is_empty() && !f.chunk_kinds.contains(kind) {
                    return Ok(false);
                }
            }
            if !f.source_kinds.is_empty()
                && c.source_kind
                    .as_ref()
                    .is_some_and(|kind| !f.source_kinds.contains(kind))
            {
                return Ok(false);
            }
            if let Some(e) = &c.entity {
                if !f.entity_ids.is_empty() && !f.entity_ids.contains(&e.id) {
                    return Ok(false);
                }
            }
            if !f.concept_ids.is_empty() && !f.concept_ids.iter().any(|id| c.concepts.contains(id))
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
#[async_trait]
impl engram_retrieval::RetrievalIndex for HelixRetrievalIndex {
    async fn retrieve_candidates(&self, q: &RetrievalRequest) -> CoreResult<Vec<RetrievalResult>> {
        if !self.store.visible_scope(&q.scope) {
            return Ok(vec![]);
        }
        let limit = q.limit.unwrap_or(50) as usize;
        if limit == 0 || limit > 1000 {
            return Err(CoreError::InvalidRequest {
                reason: "invalid bounded retrieval limit".into(),
            });
        }
        if q.query.trim().is_empty() {
            return Ok(vec![]);
        }
        if q.modes.contains(&RetrievalMode::Semantic) {
            return self.semantic(q, limit);
        }
        let partition = self.store.inner.partition.discriminator().map_err(core)?;
        let mut results = vec![];
        for family in TEXT_FAMILIES {
            let mut allowed = BTreeMap::new();
            for v in self.store.all::<Value>(family).map_err(core)? {
                if let Some(candidate) = self.project(family, &v)? {
                    if self.eligible(&candidate, q)? {
                        allowed.insert(id(&v).map_err(core)?.to_owned(), candidate);
                    }
                }
            }
            if allowed.is_empty() {
                continue;
            }
            let ids: Vec<String> = allowed.keys().cloned().collect();
            let request = QueryRequest::read(
                read_batch()
                    .var_as(
                        "rows",
                        g().n_with_label_where(
                            *family,
                            SourcePredicate::eq("partition", partition.clone()),
                        )
                        .where_(Predicate::is_in("public_id", ids))
                        .text_search(
                            *family,
                            "retrieval_text",
                            q.query.clone(),
                            limit,
                            Some(partition.clone().into()),
                        )
                        .project_bindings(vec![
                            BindingProjection::current("public_id", "public_id"),
                            BindingProjection::current("partition", "partition"),
                            BindingProjection::current("$score", "score"),
                        ]),
                    )
                    .returning(["rows"]),
            );
            let response = self
                .store
                .inner
                .transport
                .execute(&Operation::new(request).map_err(core)?)
                .map_err(core)?;
            for row in crate::schema::rows(&response, "rows").map_err(core)? {
                let id = row["public_id"]
                    .as_str()
                    .ok_or_else(|| core(Error::IncompatibleSchema))?;
                if row["partition"].as_str() != Some(partition.as_str()) {
                    return Err(core(Error::IncompatibleSchema));
                }
                let v = allowed
                    .get(id)
                    .ok_or_else(|| core(Error::IncompatibleSchema))?;
                let score = row["score"]
                    .as_f64()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| core(Error::IncompatibleSchema))?
                    as f32;
                results.push(self.result(family, id, v, score)?);
            }
        }
        results.sort_by(|a, b| {
            b.score
                .total
                .total_cmp(&a.score.total)
                .then_with(|| a.target_id.cmp(&b.target_id))
        });
        results.truncate(limit);
        Ok(results)
    }
}
