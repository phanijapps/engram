//! Associative graph retrieval behind the `RetrievalIndex` port.
//!
//! [`AssociativeGraphIndex`] ranks knowledge-graph entities by Personalized
//! PageRank seeded at the entities named in the retrieval query. It is the
//! associative/graph-walk counterpart to the lexical `GraphRetrievalIndex`:
//! both implement `RetrievalIndex` for `RetrievalMode::Graph`, and a future
//! composition slice chooses which (or both) to fan out. This index coexists
//! with the lexical one — it does not modify it.
//!
//! All graph data enters through the injected [`GraphRelationshipSource`], which
//! scope-filters before returning, so the PPR walk runs only over in-scope edges
//! and cannot cross scope boundaries.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    AllowedUse, DeleteMode, EntityRef, FusionStrategy, FusionTrace, KnowledgeEntity, Metadata,
    Policy, RerankStrategy, Retention, RetrievalRequest, RetrievalResult, RetrievalScore,
    RetrievalTargetType, Sensitivity, Visibility,
};
use engram_retrieval::RetrievalIndex;
use engram_runtime::CoreResult;
use futures::future::try_join;
use serde_json::Value;

use crate::GraphRelationshipSource;
use crate::ranking::{PprConfig, rank_associative};
use crate::seeds::resolve_seeds;

/// Source label stamped on every result's `FusionTrace`.
const SOURCE: &str = "associative_graph";

/// Associative graph retrieval: ranks knowledge-graph entities by Personalized
/// PageRank seeded at the entities named in the query.
///
/// Engine-neutral — no SQL or storage-engine type lives here. Scope isolation is
/// inherited from the injected [`GraphRelationshipSource`] (it scope-filters
/// before returning), so the walk cannot reach out-of-scope nodes.
pub struct AssociativeGraphIndex {
    source: Arc<dyn GraphRelationshipSource>,
    config: PprConfig,
    default_limit: u32,
}

impl AssociativeGraphIndex {
    /// Creates an associative graph index with default PPR config and a
    /// candidate limit of 20.
    pub fn new(source: Arc<dyn GraphRelationshipSource>) -> Self {
        Self::with_default_limit(source, 20)
    }

    /// Creates an associative graph index with an explicit fallback candidate
    /// limit (used when the request specifies none).
    pub fn with_default_limit(
        source: Arc<dyn GraphRelationshipSource>,
        default_limit: u32,
    ) -> Self {
        Self {
            source,
            config: PprConfig::default(),
            default_limit,
        }
    }

    /// Creates an associative graph index with an explicit PPR config (and the
    /// default candidate limit of 20), so the wiring slice can tune `damping`
    /// (precision) or `iterations` (latency) without reopening this struct.
    pub fn with_config(source: Arc<dyn GraphRelationshipSource>, config: PprConfig) -> Self {
        Self {
            source,
            config,
            default_limit: 20,
        }
    }
}

#[async_trait]
impl RetrievalIndex for AssociativeGraphIndex {
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        let limit = request
            .limit
            .or_else(|| request.budget.as_ref().and_then(|budget| budget.max_items))
            .unwrap_or(self.default_limit);
        let scope = &request.scope;

        // Edges and entities are scope-filtered by the source before they reach
        // the walk — this is the scope-isolation boundary.
        let (entities, relationships) = try_join(
            self.source.entities(scope),
            self.source.relationships(scope),
        )
        .await?;

        let entity_pairs: Vec<(String, String)> = entities
            .iter()
            .map(|e| (e.id.as_str().to_owned(), e.name.clone()))
            .collect();
        let seeds = resolve_seeds(&request.query, &entity_pairs);
        if seeds.is_empty() {
            // No anchor to personalize on — do not traverse.
            return Ok(Vec::new());
        }

        // Directed edges over all predicates — `rank_associative` makes the walk
        // bidirected. Endpoints resolve to their entity id only; relationships
        // whose endpoints carry no id are skipped.
        let edges: Vec<(String, String)> = relationships
            .iter()
            .filter_map(|r| {
                let s = entity_key(&r.subject)?;
                let o = entity_key(&r.object)?;
                Some((s, o))
            })
            .collect();

        let mut ranked = rank_associative(&edges, &seeds, self.config);
        ranked.truncate(limit.max(1) as usize);

        // Recover entity content/provenance by key. A ranked key with no
        // discoverable entity (e.g. a relationship endpoint absent from the
        // entity list) is skipped — this slice only emits fully describable
        // entities.
        let by_key: HashMap<&str, &KnowledgeEntity> =
            entities.iter().map(|e| (e.id.as_str(), e)).collect();
        let results = ranked
            .into_iter()
            .enumerate()
            .filter_map(|(rank, (key, ppr))| {
                by_key
                    .get(key.as_str())
                    .map(|entity| build_result(rank, &key, ppr, entity, &seeds))
            })
            .collect();
        Ok(results)
    }
}

/// Returns the entity-id node key, or `None` if the reference carries no id.
///
/// Id-only keying keeps seed keys (`entity.id.as_str()`) and edge endpoints in
/// one key space. Implemented locally — the crate does not depend on
/// `engram-codegraph-queries`.
fn entity_key(reference: &EntityRef) -> Option<String> {
    reference.id.as_ref().map(|id| id.as_str().to_owned())
}

/// Builds a `RetrievalResult` for a ranked entity, mirroring the lexical graph
/// index's result shape (score in `total`/`relevance`, `policy_fit = 1.0`,
/// `FusionTrace.source` set to [`SOURCE`]) so downstream RRF fusion treats it
/// uniformly. The resolved seed ids are stamped into `metadata` for
/// diagnosability.
fn build_result(
    rank: usize,
    key: &str,
    ppr: f64,
    entity: &KnowledgeEntity,
    seeds: &[String],
) -> RetrievalResult {
    let score = ppr as f32;
    RetrievalResult {
        id: format!("associative-graph-entity-{key}"),
        target_type: RetrievalTargetType::Entity,
        target_id: key.to_owned(),
        content: entity.name.clone(),
        score: RetrievalScore {
            total: score,
            relevance: Some(score),
            recency: None,
            confidence: None,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: entity.provenance.clone(),
        policy: graph_default_policy(),
        explanation: None,
        fusion_trace: Some(FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: SOURCE.to_owned(),
            source_rank: Some((rank + 1) as u32),
            source_score: Some(score),
            score: None,
            rank: None,
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(score),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(score),
            discard_reason: None,
            deduplicated_with: Vec::new(),
        }),
        metadata: Some(seed_metadata(seeds)),
    }
}

/// Records the resolved seed entity ids that drove a result's ranking, so an
/// operator can answer "why did this rank here" once the wiring slice lands.
fn seed_metadata(seeds: &[String]) -> Metadata {
    std::iter::once((
        "seeds".to_owned(),
        Value::Array(seeds.iter().map(|s| Value::String(s.clone())).collect()),
    ))
    .collect()
}

/// Default policy for associative-graph entity candidates (entities carry no
/// policy field), mirroring the lexical graph index.
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

#[cfg(test)]
mod tests {
    use super::{AssociativeGraphIndex, GraphRelationshipSource};
    use async_trait::async_trait;
    use chrono::Utc;
    use engram_domain::{
        Actor, ActorKind, EntityKind, EntityRef, Id, KnowledgeEntity, KnowledgeRelationship,
        Provenance, Requester, RetrievalMode, RetrievalRequest, RetrievalTargetType, Scope,
    };
    use engram_retrieval::RetrievalIndex;
    use engram_runtime::{CoreError, CoreResult};
    use futures::executor::block_on;
    use std::sync::Arc;

    /// Stub edge/entity source. It does NOT scope-filter — that is the wiring
    /// slice's responsibility; the source is test-controlled here.
    struct StubSource {
        entities: Vec<KnowledgeEntity>,
        relationships: Vec<KnowledgeRelationship>,
    }

    #[async_trait]
    impl GraphRelationshipSource for StubSource {
        async fn entities(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
            Ok(self.entities.clone())
        }
        async fn relationships(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
            Ok(self.relationships.clone())
        }
    }

    /// A source whose `entities` read always fails — used to verify errors
    /// propagate rather than becoming silent empty results.
    struct FailingSource;

    #[async_trait]
    impl GraphRelationshipSource for FailingSource {
        async fn entities(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
            Err(CoreError::Adapter {
                adapter: "associative_graph_test".to_owned(),
                message: "source failure".to_owned(),
            })
        }
        async fn relationships(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
            Ok(Vec::new())
        }
    }

    fn actor() -> Actor {
        Actor {
            id: Id::from("actor-associative-graph-test"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        }
    }

    fn scope() -> Scope {
        Scope {
            tenant: "t".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn provenance() -> Provenance {
        Provenance {
            source: "associative_graph_test".to_owned(),
            actor: actor(),
            observed_at: Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("test".to_owned()),
        }
    }

    fn ent(id: &str, name: &str) -> KnowledgeEntity {
        KnowledgeEntity {
            id: Id::from(id),
            graph_id: None,
            kind: EntityKind::Concept,
            name: name.to_owned(),
            aliases: Vec::new(),
            scope: scope(),
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: provenance(),
            created_at: Utc::now(),
            updated_at: None,
            valid_from: None,
            valid_until: None,
            metadata: None,
        }
    }

    fn ref_of(key: &str) -> EntityRef {
        EntityRef {
            id: Some(Id::from(key)),
            kind: None,
            name: Some(key.to_owned()),
            aliases: Vec::new(),
        }
    }

    fn rel(subject: &str, object: &str) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: Id::from(format!("rel-{subject}-{object}")),
            graph_id: None,
            subject: ref_of(subject),
            predicate: "related_to".to_owned(),
            object: ref_of(object),
            scope: scope(),
            evidence: Vec::new(),
            confidence: None,
            provenance: provenance(),
            created_at: Utc::now(),
            updated_at: None,
        }
    }

    fn request(query: &str, limit: Option<u32>) -> RetrievalRequest {
        RetrievalRequest {
            query: query.to_owned(),
            scope: scope(),
            requester: Requester {
                actor: actor(),
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            modes: vec![RetrievalMode::Graph],
            filters: None,
            cues: Vec::new(),
            limit,
            budget: None,
            include_explanations: Some(true),
        }
    }

    #[test]
    fn returns_ranked_entities_seeded_by_query() {
        let source = Arc::new(StubSource {
            entities: vec![
                ent("a", "Alice"),
                ent("b", "Bob"),
                ent("c", "Carol"),
                ent("d", "Dave"),
            ],
            relationships: vec![rel("a", "b"), rel("b", "c"), rel("c", "d")],
        });
        let index = AssociativeGraphIndex::new(source);
        let results = block_on(index.retrieve_candidates(&request("Alice", None))).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.target_id.as_str()).collect();
        // Assert the contractual proximity property (seed + 1-hop neighborhood
        // outrank distant nodes; the farthest ranks last), NOT the incidental
        // b>a ordering — a higher-degree neighbor can outrank the seed under PPR,
        // and that is correct but not contractual. See the ranking tests.
        for close in ["a", "b"] {
            for far in ["c", "d"] {
                let ci = ids.iter().position(|x| *x == close).unwrap();
                let fi = ids.iter().position(|x| *x == far).unwrap();
                assert!(ci < fi, "{close} should rank above {far}");
            }
        }
        assert_eq!(ids.last(), Some(&"d"));
        assert!(ids.contains(&"a"), "seed a must appear");
        for result in &results {
            assert_eq!(result.target_type, RetrievalTargetType::Entity);
            assert_eq!(
                result.fusion_trace.as_ref().unwrap().source,
                "associative_graph"
            );
        }
        // Resolved seed ids are stamped into metadata for diagnosability.
        let stamped = results[0]
            .metadata
            .as_ref()
            .expect("metadata stamped")
            .get("seeds")
            .and_then(|v| v.as_array())
            .expect("seeds array");
        let stamped_ids: Vec<&str> = stamped.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(stamped_ids, vec!["a"]);
    }

    #[test]
    fn query_matching_no_entity_returns_empty() {
        let source = Arc::new(StubSource {
            entities: vec![ent("a", "Alice")],
            relationships: Vec::new(),
        });
        let index = AssociativeGraphIndex::new(source);
        let results = block_on(index.retrieve_candidates(&request("zzz", None))).unwrap();
        assert!(results.is_empty(), "no seed -> no traversal");
    }

    #[test]
    fn limit_truncates_after_ranking() {
        let source = Arc::new(StubSource {
            entities: vec![
                ent("a", "Alice"),
                ent("b", "Bob"),
                ent("c", "Carol"),
                ent("d", "Dave"),
            ],
            relationships: vec![rel("a", "b"), rel("b", "c"), rel("c", "d")],
        });
        let index = AssociativeGraphIndex::new(source);
        let results = block_on(index.retrieve_candidates(&request("Alice", Some(2)))).unwrap();
        assert_eq!(results.len(), 2);
        // Top two are the seed's close pair {a, b}; the order between them is
        // incidental PPR behavior, not asserted.
        let top2: Vec<&str> = results.iter().map(|r| r.target_id.as_str()).collect();
        assert!(top2.contains(&"a") && top2.contains(&"b"));
    }

    #[test]
    fn walk_invents_no_edges_unconnected_entity_absent() {
        // Entities a, b, c but only one relationship a-b. Query "Alice" seeds a.
        // c has no edge and is not a seed, so it must NOT appear — the walk runs
        // only over edges the source supplied.
        let source = Arc::new(StubSource {
            entities: vec![ent("a", "Alice"), ent("b", "Bob"), ent("c", "Carol")],
            relationships: vec![rel("a", "b")],
        });
        let index = AssociativeGraphIndex::new(source);
        let results = block_on(index.retrieve_candidates(&request("Alice", None))).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.target_id.as_str()).collect();
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
        assert!(
            !ids.contains(&"c"),
            "c (no edge, not a seed) must not appear"
        );
    }

    #[test]
    fn source_error_propagates() {
        let index = AssociativeGraphIndex::new(Arc::new(FailingSource));
        let result = block_on(index.retrieve_candidates(&request("Alice", None)));
        assert!(
            result.is_err(),
            "a source failure must propagate, not silently become empty"
        );
    }
}
