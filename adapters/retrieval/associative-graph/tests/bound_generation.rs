//! TASK-014 focused proof: contributor-aware associative invalidation on the
//! REAL `AssociativeGraphIndex` (Personalized PageRank) with a shared warm
//! `GraphCache`.
//!
//! Eligibility enters through the engine-neutral `CodeEligibilityResolver`
//! port. This test drives it with a deterministic flip-fixture (a typed double
//! at that port boundary — the durable SQLite resolver driving the same lane
//! end to end is proven in `engram-conformance`'s `bound_retrieval` target).
//! The behavior under proof is the adapter's own: the PPR universe, seeds and
//! edges are constrained BEFORE the walk, and a warm snapshot materialized
//! under a retired epoch is never served again.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::{
    Actor, ActorKind, EntityKind, EntityRef, Id, KnowledgeEntity, KnowledgeRelationship,
    Provenance, Requester, RetrievalRequest, Scope,
};
use engram_retrieval::{
    BoundCodeEligibility, CodeEligibilityResolver, InMemoryGraphCache, RetrievalIndex,
};
use engram_runtime::CoreResult;
use engram_store_associative_graph::{AssociativeGraphIndex, GraphRelationshipSource, PprConfig};
use futures::executor::block_on;

// ---------- fixture ---------------------------------------------------------

/// Deterministic eligibility fixture at the resolver port: starts at
/// `epoch 1 / gen-1 published`, and flips to `epoch 2 / gen-2 published`
/// (gen-1 retired) on command — exactly what a durable publish does.
struct FlipEligibility {
    current: Mutex<BoundCodeEligibility>,
}

impl FlipEligibility {
    fn gen1() -> Self {
        Self {
            current: Mutex::new(BoundCodeEligibility {
                epoch: 1,
                generations: vec!["gen-1".to_owned()],
                repositories: vec!["repo-a".to_owned()],
            }),
        }
    }

    fn retire_gen1_publish_gen2(&self) {
        *self.current.lock().expect("flip lock") = BoundCodeEligibility {
            epoch: 2,
            generations: vec!["gen-2".to_owned()],
            repositories: vec!["repo-a".to_owned()],
        };
    }
}

#[async_trait]
impl CodeEligibilityResolver for FlipEligibility {
    async fn resolve(&self, _scope: &Scope) -> CoreResult<Option<BoundCodeEligibility>> {
        Ok(Some(self.current.lock().expect("flip lock").clone()))
    }
}

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

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-bound-generation-test"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "task-014".to_owned(),
        subject: None,
        workspace: Some("bound-generation".to_owned()),
        session: None,
        environment: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "bound-generation-test".to_owned(),
        actor: actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("test".to_owned()),
    }
}

fn entity(id: &str, name: &str, kind: EntityKind, generation: Option<&str>) -> KnowledgeEntity {
    let mut metadata = engram_domain::Metadata::new();
    if let Some(generation) = generation {
        metadata.insert(
            "codeGenerationId".to_owned(),
            serde_json::Value::String(generation.to_owned()),
        );
        metadata.insert(
            "codeRepoId".to_owned(),
            serde_json::Value::String("repo-a".to_owned()),
        );
    }
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: None,
        kind,
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
        metadata: Some(metadata),
        archived_at: None,
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

fn edge(id: &str, subject: &str, object: &str, predicate: &str) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(id),
        graph_id: None,
        predicate: predicate.to_owned(),
        subject: ref_of(subject),
        object: ref_of(object),
        scope: scope(),
        evidence: Vec::new(),
        confidence: None,
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        code_resolution: None,
        archived_at: None,
    }
}

fn request(query: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_owned(),
        scope: scope(),
        requester: Requester {
            actor: actor(),
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        },
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: None,
    }
}

/// The store KEEPS serving every record (warm store rows are not deleted by a
/// publication); only eligibility flips. This is the exact retirement shape.
fn graph() -> StubSource {
    StubSource {
        entities: vec![
            entity(
                "entity-stale",
                "staleFn",
                EntityKind::Function,
                Some("gen-1"),
            ),
            entity(
                "entity-stale-hub",
                "staleHub",
                EntityKind::Struct,
                Some("gen-1"),
            ),
            entity(
                "entity-eligible",
                "eligibleFn",
                EntityKind::Function,
                Some("gen-2"),
            ),
            // Non-code record: never stamped, always eligible.
            entity("entity-shared", "sharedDoc", EntityKind::Concept, None),
        ],
        relationships: vec![
            edge("edge-1", "entity-stale", "entity-stale-hub", "calls"),
            edge("edge-2", "entity-stale", "entity-shared", "mentions"),
            edge("edge-3", "entity-eligible", "entity-shared", "mentions"),
        ],
    }
}

fn bound_index(
    source: Arc<StubSource>,
    eligibility: Arc<FlipEligibility>,
    cache: Arc<InMemoryGraphCache>,
) -> AssociativeGraphIndex {
    AssociativeGraphIndex::with_cache_and_eligibility(
        source,
        PprConfig::default(),
        cache,
        eligibility as Arc<dyn CodeEligibilityResolver>,
    )
}

// ---------- task_014 focused cases -----------------------------------------

mod task_014 {
    use super::*;

    /// warm_generation_retirement: the warm cache snapshot materialized under
    /// gen-1 is served while gen-1 is published; after gen-1 retires (epoch
    /// bump) the SAME warm cache must never serve the retired node again — the
    /// snapshot is re-materialized from the eligible universe.
    #[test]
    fn warm_generation_retirement() {
        let eligibility = Arc::new(FlipEligibility::gen1());
        let index = bound_index(
            Arc::new(graph()),
            eligibility.clone(),
            Arc::new(InMemoryGraphCache::new()),
        );

        // Warm-up under gen-1: the stale node is legitimately ranked and its
        // snapshot is now cached under epoch 1.
        let before = block_on(index.retrieve_candidates(&request("staleFn"))).expect("warm query");
        assert!(
            before
                .iter()
                .any(|result| result.target_id == "entity-stale"),
            "gen-1 is published at epoch 1, so its nodes must rank: {:?}",
            before
                .iter()
                .map(|r| r.target_id.clone())
                .collect::<Vec<_>>()
        );

        // Retire gen-1 / publish gen-2: same warm cache handle, same store.
        eligibility.retire_gen1_publish_gen2();
        let after =
            block_on(index.retrieve_candidates(&request("staleFn"))).expect("post-retire query");
        let ids: Vec<String> = after.iter().map(|r| r.target_id.clone()).collect();
        assert!(
            !ids.iter().any(|id| id.contains("stale")),
            "a retired generation must never seed, rank or traverse after epoch bump: {ids:?}"
        );

        // The eligible generation still serves through the same lane/cache.
        let eligible =
            block_on(index.retrieve_candidates(&request("eligibleFn"))).expect("eligible query");
        assert!(
            eligible
                .iter()
                .any(|result| result.target_id == "entity-eligible"),
            "the newly published generation must rank: {:?}",
            eligible
                .iter()
                .map(|r| r.target_id.clone())
                .collect::<Vec<_>>()
        );
    }

    /// mixed_lineage_recompute: a non-code record with edges into BOTH a
    /// retired generation and a surviving one keeps serving through the
    /// surviving edge — the lane recomputes from surviving inputs instead of
    /// dropping the record wholesale or bridging through the stale edge.
    #[test]
    fn mixed_lineage_recompute() {
        let eligibility = Arc::new(FlipEligibility::gen1());
        eligibility.retire_gen1_publish_gen2();
        let index = bound_index(
            Arc::new(graph()),
            eligibility,
            Arc::new(InMemoryGraphCache::new()),
        );

        // Seed on the eligible symbol; the walk must reach the shared
        // non-code record through the SURVIVING edge (edge-3), never through
        // the retired edge-2/edge-1 chain.
        let results = block_on(index.retrieve_candidates(&request("eligibleFn"))).expect("query");
        let ids: Vec<String> = results.iter().map(|r| r.target_id.clone()).collect();
        assert!(
            ids.contains(&"entity-shared".to_owned()),
            "the non-code record must survive via its surviving edge: {ids:?}"
        );
        assert!(
            !ids.iter().any(|id| id.contains("stale")),
            "retired-generation nodes must be absent from a mixed recomputation: {ids:?}"
        );

        // Seeding on the retired symbol yields nothing at all: it cannot
        // consume the traversal budget as a bridge into the surviving graph.
        let stale = block_on(index.retrieve_candidates(&request("staleFn"))).expect("stale query");
        assert!(
            stale.is_empty(),
            "an ineligible seed must not traverse, bridge or rank: {:?}",
            stale
                .iter()
                .map(|r| r.target_id.clone())
                .collect::<Vec<_>>()
        );
    }
}
