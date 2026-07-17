//! Graph fusion and candidate retrieval operations.
//!
//! Retrieval-composition seam (RFC-0005): graph-ranked Entity/Chunk
//! candidates for requests, and reciprocal-rank fusion of candidate lists.

use async_trait::async_trait;
use engram_domain::{
    KnowledgeEntity, KnowledgeRelationship, RetrievalRequest, RetrievalResult, Scope,
};
use engram_retrieval::{
    DEFAULT_RRF_K, ReciprocalFusionConfig, ReciprocalRankFusion, RetrievalFusion, RetrievalIndex,
};
use engram_runtime::CoreResult;
use engram_store_associative_graph::{AssociativeGraphIndex, GraphRelationshipSource};
use engram_store_sqlite::{GraphCandidateSource, GraphRetrievalIndex, SqlKnowledgeStore};
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use std::sync::Arc;

use crate::{decode, encode, to_napi_error};

/// Retrieval-composition seam (RFC-0005): graph-ranked Entity/Chunk
/// candidates for a request, as `RetrievalResult` JSON tagged
/// `source = "graph"`, ready to RRF-fuse with vector candidates.
pub fn graph_candidates_json(
    store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let request = decode::<RetrievalRequest>(&request_json)?;
    let source: Arc<dyn GraphCandidateSource> = store.clone();
    let index = GraphRetrievalIndex::new(source);
    let results = block_on(index.retrieve_candidates(&request)).map_err(to_napi_error)?;
    encode(&results)
}

/// Orphan-rule wrapper adapting `SqlKnowledgeStore` to the associative-graph
/// edge source. A bare `impl GraphRelationshipSource for SqlKnowledgeStore` is
/// forbidden in this crate (neither the trait — from `engram-store-associative-
/// graph` — nor the store — from `engram-store-knowledge-sqlite` — is local to
/// `engram-node`), so this newtype is the local type the impl hangs on. The
/// lexical `GraphCandidateSource` path needs no such wrapper because
/// `SqlKnowledgeStore` implements it in its own crate.
pub(crate) struct KnowledgeRelationshipSource(pub(crate) Arc<SqlKnowledgeStore>);

#[async_trait]
impl GraphRelationshipSource for KnowledgeRelationshipSource {
    async fn entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        self.0.list_entities(scope).await
    }
    async fn relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.0.list_relationships(scope).await
    }
}

/// Retrieval-composition seam (RFC-0005): associative (Personalized PageRank)
/// graph-ranked Entity candidates for a request, as `RetrievalResult` JSON tagged
/// `source = "associative_graph"`, ready to RRF-fuse with graph/vector/lexical
/// candidates.
pub fn associative_graph_candidates_json(
    store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let request = decode::<RetrievalRequest>(&request_json)?;
    let source: Arc<dyn GraphRelationshipSource> =
        Arc::new(KnowledgeRelationshipSource(store.clone()));
    let index = AssociativeGraphIndex::new(source);
    let results = block_on(index.retrieve_candidates(&request)).map_err(to_napi_error)?;
    encode(&results)
}

/// Retrieval-composition seam (RFC-0005): reciprocal-rank fusion of
/// candidate lists (graph + vector) into one ranked list. Configurable
/// strength (`k`, per-source `weights`) with defaults when omitted.
pub fn fuse_rrf_json(_store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let request: RetrievalRequest = serde_json::from_value(value["request"].clone())
        .map_err(|e| Error::from_reason(e.to_string()))?;
    let candidates: Vec<RetrievalResult> = serde_json::from_value(value["candidates"].clone())
        .map_err(|e| Error::from_reason(e.to_string()))?;
    let k = value["k"]
        .as_u64()
        .map(|n| n as u32)
        .unwrap_or(DEFAULT_RRF_K);
    let default_weight = value["defaultWeight"]
        .as_f64()
        .map(|f| f as f32)
        .unwrap_or(1.0);
    let weights: std::collections::BTreeMap<String, f32> = value
        .get("weights")
        .and_then(|w| w.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f as f32)))
                .collect()
        })
        .unwrap_or_default();
    let config = ReciprocalFusionConfig::new(k, default_weight, weights).map_err(to_napi_error)?;
    let fused = ReciprocalRankFusion::new(config)
        .fuse(&request, candidates)
        .map_err(to_napi_error)?;
    encode(&fused)
}

/// Retrieval-composition seam (RFC-0005): reciprocal-rank fusion of ranked
/// id lists (e.g. graph chunk ids + vector chunk ids) into one fused order.
/// Lightweight alternative to `fuseRrfJson` for callers that have ranked id
/// lists, not full `RetrievalResult`s — the demo uses this to fuse graph +
/// vector chunk orders without marshaling Provenance/Policy per candidate.
/// The formula mirrors `ReciprocalRankFusion` (1/(k + rank)); the canonical,
/// tested impl lives in `engram-retrieval`.
pub fn fuse_rrf_ids_json(_store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let lists: Vec<Vec<String>> = serde_json::from_value(value["lists"].clone())
        .map_err(|e| Error::from_reason(e.to_string()))?;
    let k = value["k"]
        .as_u64()
        .map(|n| n as u32)
        .unwrap_or(DEFAULT_RRF_K) as f32;
    let limit = value["limit"].as_u64().map(|n| n as usize);
    // score(id) = Σ over lists of 1/(k + rank_in_list); first-seen order for
    // a stable tiebreak. An id in two lists is boosted (cross-source consensus).
    let mut scores: std::collections::BTreeMap<String, f32> = std::collections::BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for list in &lists {
        for (rank, id) in list.iter().enumerate() {
            let contribution = 1.0 / (k + (rank + 1) as f32);
            if scores.insert(id.clone(), 0.0).is_none() {
                order.push(id.clone());
            }
            if let Some(s) = scores.get_mut(id) {
                *s += contribution;
            }
        }
    }
    order.sort_by(|a, b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(limit) = limit {
        order.truncate(limit);
    }
    encode(&order)
}

#[cfg(test)]
mod tests {
    use super::{KnowledgeRelationshipSource, associative_graph_candidates_json};
    use chrono::Utc;
    use engram_domain::{
        Actor, ActorKind, EntityKind, EntityRef, Id, KnowledgeEntity, KnowledgeRelationship,
        Provenance, Requester, RetrievalMode, RetrievalRequest, RetrievalResult,
        RetrievalTargetType, Scope,
    };
    use engram_knowledge::KnowledgeRepository;
    use engram_store_associative_graph::GraphRelationshipSource;
    use engram_store_sqlite::SqlKnowledgeStore;
    use futures::executor::block_on;
    use std::sync::Arc;

    fn actor() -> Actor {
        Actor {
            id: Id::from("actor-associative-graph-wiring-test"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        }
    }

    fn scope(tenant: &str) -> Scope {
        Scope {
            tenant: tenant.to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn provenance() -> Provenance {
        Provenance {
            source: "associative_graph_wiring_test".to_owned(),
            actor: actor(),
            observed_at: Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("test".to_owned()),
        }
    }

    fn ent(id: &str, name: &str, tenant: &str) -> KnowledgeEntity {
        KnowledgeEntity {
            id: Id::from(id),
            graph_id: None,
            kind: EntityKind::Concept,
            name: name.to_owned(),
            aliases: Vec::new(),
            scope: scope(tenant),
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

    fn rel(subject: &str, object: &str, tenant: &str) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: Id::from(format!("rel-{subject}-{object}")),
            graph_id: None,
            subject: ref_of(subject),
            predicate: "related_to".to_owned(),
            object: ref_of(object),
            scope: scope(tenant),
            evidence: Vec::new(),
            confidence: None,
            provenance: provenance(),
            created_at: Utc::now(),
            updated_at: None,
        }
    }

    fn request(query: &str, tenant: &str) -> RetrievalRequest {
        RetrievalRequest {
            query: query.to_owned(),
            scope: scope(tenant),
            requester: Requester {
                actor: actor(),
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            modes: vec![RetrievalMode::Graph],
            filters: None,
            cues: Vec::new(),
            limit: None,
            budget: None,
            include_explanations: Some(true),
        }
    }

    #[test]
    fn associative_candidates_over_seeded_store() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        // Tenant "t": chain a - b - c - d, plus an isolated in-scope entity w.
        for (id, name) in [
            ("a", "Alice"),
            ("b", "Bob"),
            ("c", "Carol"),
            ("d", "Dave"),
            ("w", "Wendy"),
        ] {
            block_on(store.put_entity(ent(id, name, "t"))).unwrap();
        }
        for (s, o) in [("a", "b"), ("b", "c"), ("c", "d")] {
            block_on(store.put_relationship(rel(s, o, "t"))).unwrap();
        }
        // Cross-scope relationship a -> w (scope "other"). The seed `a` is
        // in-scope and `w` is an in-scope entity, so the walk WOULD reach `w` if
        // this relationship were admitted. Its absence proves the relationship
        // scope filter at the read boundary (the wrapper delegates to
        // scope-filtered `list_relationships`).
        block_on(store.put_relationship(rel("a", "w", "other"))).unwrap();

        let req_json = serde_json::to_string(&request("Alice", "t")).unwrap();
        let out = associative_graph_candidates_json(&Arc::new(store), req_json).unwrap();
        let results: Vec<RetrievalResult> = serde_json::from_str(&out).unwrap();

        assert!(!results.is_empty(), "seeded query must return candidates");
        assert!(
            results
                .iter()
                .all(|r| r.target_type == RetrievalTargetType::Entity),
            "all candidates are entities"
        );
        assert!(
            results.iter().all(|r| r
                .fusion_trace
                .as_ref()
                .map(|t| t.source == "associative_graph")
                .unwrap_or(false)),
            "all candidates carry the associative_graph source tag"
        );
        let ids: Vec<&str> = results.iter().map(|r| r.target_id.as_str()).collect();
        assert!(ids.contains(&"a"), "seed a present");
        assert!(ids.contains(&"b"), "1-hop neighbor b present");
        assert!(
            !ids.contains(&"w"),
            "cross-scope relationship a->w must be filtered out, so w is unreachable"
        );
        // Proximity: seed + 1-hop neighborhood {a, b} outrank {c, d}; d last.
        let pos = |id: &str| ids.iter().position(|x| *x == id).unwrap();
        for close in ["a", "b"] {
            for far in ["c", "d"] {
                assert!(pos(close) < pos(far), "{close} should rank above {far}");
            }
        }
        assert_eq!(ids.last(), Some(&"d"));
    }

    #[test]
    fn no_seed_returns_empty() {
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        block_on(store.put_entity(ent("a", "Alice", "t"))).unwrap();
        let req_json = serde_json::to_string(&request("zzz", "t")).unwrap();
        let out = associative_graph_candidates_json(&Arc::new(store), req_json).unwrap();
        let results: Vec<RetrievalResult> = serde_json::from_str(&out).unwrap();
        assert!(results.is_empty(), "no seed -> no traversal");
    }

    #[test]
    fn wrapper_delegates_to_scope_filtered_store_reads() {
        // The newtype must return exactly the store's scope-filtered
        // `list_entities` / `list_relationships` output — no more, no less.
        let store = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
        block_on(store.put_entity(ent("a", "Alice", "t"))).unwrap();
        block_on(store.put_entity(ent("z", "Zed", "other"))).unwrap();
        block_on(store.put_relationship(rel("a", "b", "t"))).unwrap();
        block_on(store.put_relationship(rel("a", "z", "other"))).unwrap();
        let wrapper = KnowledgeRelationshipSource(store.clone());
        let s = scope("t");
        let ents = block_on(wrapper.entities(&s)).unwrap();
        let rels = block_on(wrapper.relationships(&s)).unwrap();
        assert_eq!(ents, block_on(store.list_entities(&s)).unwrap());
        assert_eq!(rels, block_on(store.list_relationships(&s)).unwrap());
        // Scope filtering is in effect: only the tenant-"t" entity/relationship.
        assert!(ents.iter().all(|e| e.id.as_str() == "a"), "z filtered out");
        assert_eq!(rels.len(), 1, "cross-scope a->z relationship filtered out");
    }
}
