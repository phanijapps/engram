//! TASK-014 focused proof: contributor-aware community-summary invalidation
//! on the REAL `CommunitySummaryIndex` (Louvain + summary match gate) with a
//! shared warm `GraphCache` (entities, relationships AND cached labels).
//!
//! Eligibility enters through the engine-neutral `CodeEligibilityResolver`
//! port. This test drives it with a deterministic flip-fixture (a typed double
//! at that port boundary — the durable SQLite resolver driving the same lane
//! end to end is proven in `engram-conformance`'s `bound_retrieval` target).
//! The behavior under proof is the adapter's own: summaries touching a retired
//! generation are recomputed from the surviving eligible code + non-code
//! inputs before serving, and a warm snapshot/label cache from a retired epoch
//! is never served.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::{
    Actor, ActorKind, EntityKind, EntityRef, Id, KnowledgeEntity, KnowledgeRelationship,
    Provenance, Requester, RetrievalRequest, Scope,
};
use engram_retrieval::{
    BoundBinding, BoundCodeEligibility, CodeEligibilityResolver, DurableLineage,
    InMemoryGraphCache, RetrievalIndex,
};
use engram_runtime::CoreResult;
use engram_store_associative_graph::GraphRelationshipSource;
use engram_store_community_summary::CommunitySummaryIndex;
use futures::executor::block_on;

// ---------- fixture ---------------------------------------------------------

/// Deterministic eligibility fixture at the resolver port: epoch 1 publishes
/// gen-1; the flip retires gen-1 and publishes gen-2.
struct FlipEligibility {
    current: Mutex<BoundCodeEligibility>,
}

impl FlipEligibility {
    fn gen1() -> Self {
        Self {
            current: Mutex::new(BoundCodeEligibility {
                epoch: 1,
                bindings: vec![BoundBinding {
                    repo_id: "repo-a".to_owned(),
                    branch_ref: "refs/heads/main".to_owned(),
                    generation_id: "gen-1".to_owned(),
                }],
                lineage: durable_lineage(),
            }),
        }
    }

    fn retire_gen1_publish_gen2(&self) {
        *self.current.lock().expect("flip lock") = BoundCodeEligibility {
            epoch: 2,
            bindings: vec![BoundBinding {
                repo_id: "repo-a".to_owned(),
                branch_ref: "refs/heads/main".to_owned(),
                generation_id: "gen-2".to_owned(),
            }],
            lineage: durable_lineage(),
        };
    }
}

/// Durable lineage: gen-1 owns the stale members, gen-2 owns the survivor.
fn durable_lineage() -> Vec<DurableLineage> {
    vec![
        DurableLineage {
            record_id: "entity-stale".to_owned(),
            repo_id: "repo-a".to_owned(),
            generation_id: "gen-1".to_owned(),
        },
        DurableLineage {
            record_id: "entity-stale-2".to_owned(),
            repo_id: "repo-a".to_owned(),
            generation_id: "gen-1".to_owned(),
        },
        DurableLineage {
            record_id: "entity-eligible".to_owned(),
            repo_id: "repo-a".to_owned(),
            generation_id: "gen-2".to_owned(),
        },
    ]
}

#[async_trait]
impl CodeEligibilityResolver for FlipEligibility {
    async fn resolve(&self, _scope: &Scope) -> CoreResult<BoundCodeEligibility> {
        Ok(self.current.lock().expect("flip lock").clone())
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
        id: Id::from("actor-mixed-lineage-test"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "task-014".to_owned(),
        subject: None,
        workspace: Some("mixed-lineage".to_owned()),
        session: None,
        environment: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "mixed-lineage-test".to_owned(),
        actor: actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("test".to_owned()),
    }
}

fn entity(id: &str, name: &str, kind: EntityKind, _generation: Option<&str>) -> KnowledgeEntity {
    // Corrective r2 (finding 4): metadata is NOT authority; the fixture's
    // durable lineage names the code records.
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
        metadata: None,
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

/// One mixed community: retired-code members, a surviving-code member and two
/// non-code members, all wired into a single clique so Louvain detects ONE
/// community while gen-1 is published.
fn mixed_graph() -> StubSource {
    StubSource {
        entities: vec![
            entity(
                "entity-stale",
                "staleSymbol",
                EntityKind::Function,
                Some("gen-1"),
            ),
            entity(
                "entity-stale-2",
                "staleSymbolPeer",
                EntityKind::Struct,
                Some("gen-1"),
            ),
            entity(
                "entity-eligible",
                "liveSymbol",
                EntityKind::Function,
                Some("gen-2"),
            ),
            entity("entity-doc", "releaseNotes", EntityKind::Concept, None),
            entity("entity-doc-2", "designMemo", EntityKind::Concept, None),
        ],
        relationships: vec![
            edge("e1", "entity-stale", "entity-stale-2", "calls"),
            edge("e2", "entity-stale", "entity-doc", "mentions"),
            edge("e3", "entity-stale-2", "entity-doc-2", "mentions"),
            edge("e4", "entity-eligible", "entity-doc", "mentions"),
            edge("e5", "entity-doc", "entity-doc-2", "related_to"),
        ],
    }
}

fn bound_index(
    source: Arc<StubSource>,
    eligibility: Arc<FlipEligibility>,
    cache: Arc<InMemoryGraphCache>,
) -> CommunitySummaryIndex {
    CommunitySummaryIndex::with_cache_and_eligibility(
        source,
        20,
        cache,
        eligibility as Arc<dyn CodeEligibilityResolver>,
    )
}

// ---------- task_014 focused cases -----------------------------------------

mod task_014 {
    use super::*;

    /// warm_generation_retirement: labels + snapshot cached under gen-1 are
    /// served while gen-1 is published; after the retirement the SAME warm
    /// cache never serves a summary naming the retired symbol — a query whose
    /// only match was the retired member finds nothing.
    #[test]
    fn warm_generation_retirement() {
        let eligibility = Arc::new(FlipEligibility::gen1());
        let index = bound_index(
            Arc::new(mixed_graph()),
            eligibility.clone(),
            Arc::new(InMemoryGraphCache::new()),
        );

        // Warm-up under gen-1: the retired symbol is legitimately served and
        // its snapshot + Louvain labels are now cached under epoch 1.
        let before =
            block_on(index.retrieve_candidates(&request("staleSymbol"))).expect("warm query");
        assert!(
            !before.is_empty(),
            "gen-1 is published at epoch 1, so its members must serve"
        );

        // Retire gen-1 / publish gen-2: same warm cache handle, same store.
        eligibility.retire_gen1_publish_gen2();
        let after =
            block_on(index.retrieve_candidates(&request("staleSymbol"))).expect("post-retire");
        assert!(
            after.is_empty(),
            "a query matching only a retired member must find no (stale) summary: {:?}",
            after.iter().map(|r| r.content.clone()).collect::<Vec<_>>()
        );

        // The surviving generation's members still serve through the lane.
        let live = block_on(index.retrieve_candidates(&request("liveSymbol"))).expect("live query");
        assert!(
            live.iter()
                .any(|result| result.target_id == "entity-eligible"),
            "the newly published generation must serve: {:?}",
            live.iter().map(|r| r.target_id.clone()).collect::<Vec<_>>()
        );
    }

    /// mixed_lineage_recompute: a community with retired-code AND non-code
    /// members is recomputed from the surviving inputs (never dropped
    /// wholesale, never served as the old mixed summary): the non-code
    /// members remain reachable and the retired member's name is absent from
    /// every served summary.
    #[test]
    fn mixed_lineage_recompute() {
        let eligibility = Arc::new(FlipEligibility::gen1());
        eligibility.retire_gen1_publish_gen2();
        let index = bound_index(
            Arc::new(mixed_graph()),
            eligibility,
            Arc::new(InMemoryGraphCache::new()),
        );

        // Query on a NON-code member that shared its community with retired
        // code: the community must still serve, recomputed from the
        // surviving member set.
        let results = block_on(index.retrieve_candidates(&request("releaseNotes"))).expect("query");
        let ids: Vec<String> = results.iter().map(|r| r.target_id.clone()).collect();
        assert!(
            ids.iter()
                .any(|id| id == "entity-doc" || id == "entity-doc-2" || id == "entity-eligible"),
            "the mixed community's surviving members must serve after recompute: {ids:?}"
        );
        assert!(
            !ids.iter().any(|id| id.contains("stale")),
            "a recomputed summary never carries retired members: {ids:?}"
        );
        // And no SERVED content names the retired symbol either.
        assert!(
            results
                .iter()
                .all(|result| !result.content.contains("staleSymbol")),
            "summary text must be recomputed from surviving inputs only: {:?}",
            results
                .iter()
                .map(|r| r.content.clone())
                .collect::<Vec<_>>()
        );
    }
}
