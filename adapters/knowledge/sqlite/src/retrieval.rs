//! Graph candidate retrieval over the knowledge store.
//!
//! Adapts knowledge-graph entities + chunks into portable `RetrievalResult`s so
//! they can RRF-fuse with vector candidates. This is the graph retriever behind
//! the `RetrievalIndex` port — its mechanism is lexical/structural (term match
//! on entity names + chunk text), but the port stays mechanism-agnostic, so a
//! future Neo4j or pgvector-graph backend swaps in by implementing the same port.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    AllowedUse, DeleteMode, FusionStrategy, FusionTrace, KnowledgeChunk, KnowledgeEntity, Policy,
    Provenance, RerankStrategy, Retention, RetrievalRequest, RetrievalResult, RetrievalScore,
    RetrievalTargetType, Scope, Sensitivity, Visibility,
};
use engram_retrieval::RetrievalIndex;
use engram_runtime::CoreResult;
use futures::future::try_join;

use crate::SqlKnowledgeStore;

/// Injected access to the graph store's entities + chunks for a scope.
///
/// The trait lets `GraphRetrievalIndex` be tested without SQLite (a stub source
/// stands in) and lets a future backend (Neo4j, pgvector-graph) plug in by
/// implementing the same two reads.
#[async_trait]
pub trait GraphCandidateSource: Send + Sync {
    async fn entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>>;
    async fn chunks(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeChunk>>;
}

/// Graph retriever behind the `RetrievalIndex` port.
///
/// Fetches entities + chunks visible to the request scope, ranks them by
/// query-term relevance, and emits `RetrievalResult`s tagged `source = "graph"`
/// for downstream RRF fusion with vector candidates.
pub struct GraphRetrievalIndex {
    source: Arc<dyn GraphCandidateSource>,
    default_limit: u32,
}

impl GraphRetrievalIndex {
    /// Creates a graph retrieval index with the default candidate limit (20).
    pub fn new(source: Arc<dyn GraphCandidateSource>) -> Self {
        Self::with_default_limit(source, 20)
    }

    /// Creates a graph retrieval index with an explicit fallback limit.
    pub fn with_default_limit(source: Arc<dyn GraphCandidateSource>, default_limit: u32) -> Self {
        Self {
            source,
            default_limit,
        }
    }
}

#[async_trait]
impl RetrievalIndex for GraphRetrievalIndex {
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        let limit = request
            .limit
            .or_else(|| request.budget.as_ref().and_then(|budget| budget.max_items))
            .unwrap_or(self.default_limit);
        let scope = &request.scope;
        let (entities, chunks) =
            try_join(self.source.entities(scope), self.source.chunks(scope)).await?;

        let mut views = Vec::with_capacity(entities.len() + chunks.len());
        for e in &entities {
            views.push(GraphCandidateView {
                target_type: RetrievalTargetType::Entity,
                target_id: e.id.as_str().to_owned(),
                text: e.name.clone(),
                provenance: e.provenance.clone(),
                policy: graph_default_policy(),
            });
        }
        for c in &chunks {
            views.push(GraphCandidateView {
                target_type: RetrievalTargetType::Chunk,
                target_id: c.id.as_str().to_owned(),
                text: c.text.clone(),
                provenance: c.provenance.clone(),
                policy: c.policy.clone(),
            });
        }
        Ok(rank_graph_candidates(&request.query, views, limit))
    }
}

/// One owned graph candidate — the pure ranking function's input + the test unit.
pub(crate) struct GraphCandidateView {
    pub(crate) target_type: RetrievalTargetType,
    pub(crate) target_id: String,
    pub(crate) text: String,
    pub(crate) provenance: Provenance,
    pub(crate) policy: Policy,
}

/// Terms used for graph matching: lowercased alphanumeric tokens, len > 2.
fn query_terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 2)
        .map(|t| t.to_owned())
        .collect()
}

/// Pure: rank graph candidates by query-term relevance and build `RetrievalResult`s.
///
/// Entity names score by match quality (exact=3, prefix/suffix=2, substring=1,
/// summed across terms); chunk text scores by summed term-hit weight. Zero-score
/// candidates are dropped (they add noise to fusion). Output is ranked and capped;
/// `source_rank` records each result's position for RRF.
pub(crate) fn rank_graph_candidates(
    query: &str,
    candidates: Vec<GraphCandidateView>,
    limit: u32,
) -> Vec<RetrievalResult> {
    let terms = query_terms(query);
    let mut scored: Vec<(f32, GraphCandidateView)> = candidates
        .into_iter()
        .map(|c| {
            let s = term_score(&terms, &c.text);
            (s, c)
        })
        .filter(|(s, _)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit.max(1) as usize);
    scored
        .into_iter()
        .enumerate()
        .map(|(rank, (score, view))| graph_result(rank, score, view))
        .collect()
}

fn term_score(terms: &[String], text: &str) -> f32 {
    if terms.is_empty() {
        return 0.0;
    }
    let lower = text.to_lowercase();
    terms
        .iter()
        .map(|t| {
            if lower == *t {
                3.0
            } else if lower.starts_with(t) || lower.ends_with(t) {
                2.0
            } else if lower.contains(t) {
                1.0
            } else {
                0.0
            }
        })
        .sum()
}

fn graph_result(rank: usize, score: f32, view: GraphCandidateView) -> RetrievalResult {
    let kind_slug = match view.target_type {
        RetrievalTargetType::Entity => "entity",
        RetrievalTargetType::Chunk => "chunk",
        _ => "item",
    };
    RetrievalResult {
        id: format!("graph-{kind_slug}-{}", view.target_id),
        target_type: view.target_type,
        target_id: view.target_id,
        content: view.text,
        score: RetrievalScore {
            total: score,
            relevance: Some(score),
            recency: None,
            confidence: None,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: view.provenance,
        policy: view.policy,
        explanation: None,
        fusion_trace: Some(FusionTrace {
            source: "graph".to_owned(),
            source_rank: Some((rank + 1) as u32),
            source_score: Some(score),
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(score),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(score),
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}

/// Default policy for graph entity candidates (entities carry no policy field).
fn graph_default_policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

/// The SQLite knowledge store is a graph candidate source.
#[async_trait]
impl GraphCandidateSource for SqlKnowledgeStore {
    async fn entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        SqlKnowledgeStore::list_entities(self, scope).await
    }
    async fn chunks(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeChunk>> {
        SqlKnowledgeStore::list_chunks(self, scope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use engram_domain::{Actor, ActorKind, Id};

    fn prov() -> Provenance {
        Provenance {
            source: "test".to_owned(),
            actor: Actor {
                id: Id::from("actor-test"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            observed_at: Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("test".to_owned()),
        }
    }

    fn view(target_type: RetrievalTargetType, id: &str, text: &str) -> GraphCandidateView {
        GraphCandidateView {
            target_type,
            target_id: id.to_owned(),
            text: text.to_owned(),
            provenance: prov(),
            policy: graph_default_policy(),
        }
    }

    #[test]
    fn exact_name_outranks_prefix_outranks_dropped() {
        // Query "Terminal" -> term "terminal": exact on "Terminal", prefix on
        // "TerminalHandle", no match on "Render" (dropped).
        let candidates = vec![
            view(RetrievalTargetType::Entity, "render", "Render"), // no match -> dropped
            view(RetrievalTargetType::Entity, "th", "TerminalHandle"), // prefix = 2
            view(RetrievalTargetType::Entity, "term", "Terminal"), // exact = 3
        ];
        let results = rank_graph_candidates("Terminal", candidates, 10);
        let ids: Vec<&str> = results.iter().map(|r| r.target_id.as_str()).collect();
        assert_eq!(ids, ["term", "th"], "exact > prefix; non-match dropped");
        assert_eq!(results[0].fusion_trace.as_ref().unwrap().source, "graph");
        assert_eq!(
            results[0].fusion_trace.as_ref().unwrap().source_rank,
            Some(1)
        );
    }

    #[test]
    fn chunk_term_match_ranks_and_is_typed_chunk() {
        let candidates = vec![
            view(
                RetrievalTargetType::Chunk,
                "c1",
                "The renderer paints text to the screen",
            ),
            view(RetrievalTargetType::Chunk, "c2", "unrelated content here"),
        ];
        let results = rank_graph_candidates("renderer text", candidates, 10);
        assert_eq!(results[0].target_id, "c1");
        assert_eq!(results[0].target_type, RetrievalTargetType::Chunk);
        // c2 has no matching term -> dropped
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn limit_caps_results() {
        let candidates = vec![
            view(RetrievalTargetType::Entity, "a", "alpha"),
            view(RetrievalTargetType::Entity, "b", "alpha"),
            view(RetrievalTargetType::Entity, "c", "alpha"),
        ];
        let results = rank_graph_candidates("alpha", candidates, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn empty_query_returns_nothing() {
        let candidates = vec![view(RetrievalTargetType::Entity, "a", "alpha")];
        let results = rank_graph_candidates("", candidates, 10);
        assert!(results.is_empty(), "no query terms => no matches");
    }
}
