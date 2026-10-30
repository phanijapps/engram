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
            retired: false,
            source_path: String::new(),
            span: None,
        },
        DurableLineage {
            record_id: "entity-stale-2".to_owned(),
            repo_id: "repo-a".to_owned(),
            generation_id: "gen-1".to_owned(),
            retired: false,
            source_path: String::new(),
            span: None,
        },
        DurableLineage {
            record_id: "entity-eligible".to_owned(),
            repo_id: "repo-a".to_owned(),
            generation_id: "gen-2".to_owned(),
            retired: false,
            source_path: String::new(),
            span: None,
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

/// A request carrying repository narrowing (`code_index_keys`).
fn request_narrowed(query: &str, keys: Option<Vec<String>>) -> RetrievalRequest {
    let mut request = request(query);
    request.filters = Some(engram_domain::QueryFilter {
        memory_kinds: Vec::new(),
        source_kinds: Vec::new(),
        chunk_kinds: Vec::new(),
        concept_ids: Vec::new(),
        entity_ids: Vec::new(),
        since: None,
        until: None,
        code_index_keys: keys,
        min_confidence: None,
        include_archived: None,
        as_of: None,
        include_code_artifacts: None,
    });
    request
}

/// Like [`entity`] but carrying metadata stamps — FORGED/null/foreign
/// variations used to prove stamps are never classification authority.
fn stamped_entity(
    id: &str,
    name: &str,
    kind: EntityKind,
    generation_stamp: Option<serde_json::Value>,
    repo_stamp: Option<serde_json::Value>,
) -> KnowledgeEntity {
    let mut record = entity(id, name, kind, None);
    let mut metadata = engram_domain::Metadata::new();
    if let Some(generation) = generation_stamp {
        metadata.insert("codeGenerationId".to_owned(), generation);
    }
    if let Some(repo) = repo_stamp {
        metadata.insert("codeRepoId".to_owned(), repo);
    }
    record.metadata = Some(metadata);
    record
}

/// Deterministic two-repository fixture: repo-a and repo-b each publish one
/// generation; lineage maps each repo's symbols to its generation. A shared
/// ordinary concept connects both sides.
struct TwoRepoEligibility {
    current: Mutex<BoundCodeEligibility>,
}

impl TwoRepoEligibility {
    fn published() -> Self {
        Self {
            current: Mutex::new(BoundCodeEligibility {
                epoch: 7,
                bindings: vec![
                    BoundBinding {
                        repo_id: "repo-a".to_owned(),
                        branch_ref: "refs/heads/main".to_owned(),
                        generation_id: "gen-a".to_owned(),
                    },
                    BoundBinding {
                        repo_id: "repo-b".to_owned(),
                        branch_ref: "refs/heads/main".to_owned(),
                        generation_id: "gen-b".to_owned(),
                    },
                ],
                lineage: vec![
                    DurableLineage {
                        record_id: "a-alpha".to_owned(),
                        repo_id: "repo-a".to_owned(),
                        generation_id: "gen-a".to_owned(),
                        retired: false,
                        source_path: String::new(),
                        span: None,
                    },
                    DurableLineage {
                        record_id: "a-beta".to_owned(),
                        repo_id: "repo-a".to_owned(),
                        generation_id: "gen-a".to_owned(),
                        retired: false,
                        source_path: String::new(),
                        span: None,
                    },
                    DurableLineage {
                        record_id: "b-gamma".to_owned(),
                        repo_id: "repo-b".to_owned(),
                        generation_id: "gen-b".to_owned(),
                        retired: false,
                        source_path: String::new(),
                        span: None,
                    },
                    DurableLineage {
                        record_id: "b-delta".to_owned(),
                        repo_id: "repo-b".to_owned(),
                        generation_id: "gen-b".to_owned(),
                        retired: false,
                        source_path: String::new(),
                        span: None,
                    },
                ],
            }),
        }
    }
}

#[async_trait]
impl CodeEligibilityResolver for TwoRepoEligibility {
    async fn resolve(&self, _scope: &Scope) -> CoreResult<BoundCodeEligibility> {
        Ok(self.current.lock().expect("two-repo lock").clone())
    }
}

fn two_repo_graph() -> StubSource {
    StubSource {
        entities: vec![
            entity("a-alpha", "alphaParser", EntityKind::Function, None),
            entity("a-beta", "betaParser", EntityKind::Function, None),
            entity("b-gamma", "gammaRender", EntityKind::Function, None),
            entity("b-delta", "deltaRender", EntityKind::Function, None),
            entity("shared-doc", "sharedNotes", EntityKind::Concept, None),
        ],
        relationships: vec![
            edge("t1", "a-alpha", "a-beta", "calls"),
            edge("t2", "a-beta", "shared-doc", "mentions"),
            edge("t3", "b-gamma", "b-delta", "calls"),
            edge("t4", "b-delta", "shared-doc", "mentions"),
        ],
    }
}

fn two_repo_index(
    source: Arc<StubSource>,
    eligibility: Arc<TwoRepoEligibility>,
    cache: Arc<InMemoryGraphCache>,
) -> CommunitySummaryIndex {
    CommunitySummaryIndex::with_cache_and_eligibility(
        source,
        20,
        cache,
        eligibility as Arc<dyn CodeEligibilityResolver>,
    )
}

/// Cold-cache reference result for one narrowing (fresh index + fresh cache).
fn cold_result(
    source: Arc<StubSource>,
    eligibility: Arc<TwoRepoEligibility>,
    keys: Option<Vec<String>>,
    query: &str,
) -> Vec<String> {
    let index = two_repo_index(source, eligibility, Arc::new(InMemoryGraphCache::new()));
    block_on(index.retrieve_candidates(&request_narrowed(query, keys)))
        .expect("cold query")
        .iter()
        .map(|result| result.content.clone())
        .collect()
}

// ---------- task_014 focused cases -----------------------------------------

mod task_014 {

    /// Corrective r2 (finding 9): the shared warm snapshot is
    /// request-INDEPENDENT raw graph data. A repo-A-narrowed request, a
    /// wildcard request and a repo-B-narrowed request at ONE unchanged epoch
    /// each return exactly their cold-cache result — in BOTH orders
    /// (A→wildcard→B and wildcard→A→B). A narrowed snapshot or label cache
    /// keyed only by scope+epoch would leak repo A's universe into repo B's
    /// answer (or vice versa).
    #[test]
    fn warm_cache_narrowing_matches_cold_cache() {
        let eligibility = Arc::new(TwoRepoEligibility::published());

        // A→wildcard→B over one shared warm cache.
        let shared = Arc::new(InMemoryGraphCache::new());
        let warm = two_repo_index(Arc::new(two_repo_graph()), eligibility.clone(), shared);
        let a_warm = block_on(
            warm.retrieve_candidates(&request_narrowed("parser", Some(vec!["repo-a".to_owned()]))),
        )
        .expect("warm repo-a query");
        let wildcard_warm = block_on(warm.retrieve_candidates(&request_narrowed("parser", None)))
            .expect("warm wildcard query");
        let b_warm = block_on(
            warm.retrieve_candidates(&request_narrowed("parser", Some(vec!["repo-b".to_owned()]))),
        )
        .expect("warm repo-b query");
        let assert_no_cross = |content: &str, forbidden: &str, ctx: &str| {
            assert!(
                !content.contains(forbidden),
                "{ctx} must not serve the other repository's symbols: {content}"
            );
        };
        for result in &a_warm {
            assert_no_cross(&result.content, "gammaRender", "repo-a narrowing");
            assert_no_cross(&result.content, "deltaRender", "repo-a narrowing");
        }
        for result in &b_warm {
            assert_no_cross(&result.content, "alphaParser", "repo-b narrowing");
            assert_no_cross(&result.content, "betaParser", "repo-b narrowing");
        }
        assert!(
            wildcard_warm
                .iter()
                .any(|result| result.content.contains("sharedNotes")),
            "the wildcard request sees the shared ordinary concept: {:?}",
            wildcard_warm
                .iter()
                .map(|r| r.content.clone())
                .collect::<Vec<_>>()
        );
        // Exact cold-cache equivalence, per request.
        let a_cold = cold_result(
            Arc::new(two_repo_graph()),
            eligibility.clone(),
            Some(vec!["repo-a".to_owned()]),
            "parser",
        );
        let wildcard_cold = cold_result(
            Arc::new(two_repo_graph()),
            eligibility.clone(),
            None,
            "parser",
        );
        let b_cold = cold_result(
            Arc::new(two_repo_graph()),
            eligibility.clone(),
            Some(vec!["repo-b".to_owned()]),
            "parser",
        );
        let contents = |results: &Vec<engram_domain::RetrievalResult>| {
            results
                .iter()
                .map(|r| r.content.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(contents(&a_warm), a_cold, "A→wildcard→B: A equals cold");
        assert_eq!(
            contents(&wildcard_warm),
            wildcard_cold,
            "A→wildcard→B: wildcard equals cold"
        );
        assert_eq!(contents(&b_warm), b_cold, "A→wildcard→B: B equals cold");

        // wildcard→A→B over a FRESH shared warm cache.
        let shared2 = Arc::new(InMemoryGraphCache::new());
        let warm2 = two_repo_index(Arc::new(two_repo_graph()), eligibility.clone(), shared2);
        let wildcard_first = block_on(warm2.retrieve_candidates(&request_narrowed("parser", None)))
            .expect("wildcard first");
        let a_after = block_on(
            warm2.retrieve_candidates(&request_narrowed("parser", Some(vec!["repo-a".to_owned()]))),
        )
        .expect("A after wildcard");
        let b_after = block_on(
            warm2.retrieve_candidates(&request_narrowed("parser", Some(vec!["repo-b".to_owned()]))),
        )
        .expect("B after wildcard");
        assert_eq!(
            contents(&wildcard_first),
            wildcard_cold,
            "wildcard→A→B: wildcard equals cold"
        );
        assert_eq!(
            contents(&a_after),
            a_cold,
            "wildcard→A→B: A equals cold after a wildcard populated the cache"
        );
        assert_eq!(
            contents(&b_after),
            b_cold,
            "wildcard→A→B: B equals cold after a wildcard populated the cache"
        );
    }

    /// Corrective r2 (finding 4): FORGED, null, foreign and missing metadata
    /// stamps on LINEAGE-FREE code entities never make them eligible — none
    /// may seed, rank or shape community summaries — while the ordinary
    /// non-code counterpart keeps serving.
    #[test]
    fn forged_and_missing_stamps_never_shape_communities() {
        let eligibility = Arc::new(FlipEligibility::gen1());
        // Baseline: the eligible universe WITHOUT the stamped forgeries.
        let baseline = Arc::new(mixed_graph());
        let baseline_index = bound_index(
            baseline,
            eligibility.clone(),
            Arc::new(InMemoryGraphCache::new()),
        );
        let baseline_results =
            block_on(baseline_index.retrieve_candidates(&request("releaseNotes")))
                .expect("baseline query");

        // The same universe PLUS four lineage-free Functions with
        // forged/null/foreign/missing stamps, wired ahead of the valid
        // members so they would dominate an unsound classifier.
        let mut entities = mixed_graph().entities;
        let mut relationships = mixed_graph().relationships;
        entities.push(stamped_entity(
            "forged-fn",
            "forgedParser",
            EntityKind::Function,
            Some(serde_json::Value::String("gen-1".to_owned())),
            Some(serde_json::Value::String("repo-a".to_owned())),
        ));
        entities.push(stamped_entity(
            "null-stamp-fn",
            "nullParser",
            EntityKind::Function,
            Some(serde_json::Value::Null),
            Some(serde_json::Value::Null),
        ));
        entities.push(stamped_entity(
            "foreign-stamp-fn",
            "foreignParser",
            EntityKind::Function,
            Some(serde_json::Value::String("gen-foreign".to_owned())),
            Some(serde_json::Value::String("repo-foreign".to_owned())),
        ));
        entities.push(stamped_entity(
            "missing-stamp-fn",
            "missingParser",
            EntityKind::Function,
            None,
            None,
        ));
        for (id, name) in [
            ("forged-fn", "forgedParser"),
            ("null-stamp-fn", "nullParser"),
            ("foreign-stamp-fn", "foreignParser"),
            ("missing-stamp-fn", "missingParser"),
        ] {
            relationships.push(edge(&format!("stamp-{id}"), id, "entity-doc", "mentions"));
            let _ = name;
        }
        let polluted = Arc::new(StubSource {
            entities,
            relationships,
        });
        let polluted_index = bound_index(
            polluted,
            eligibility.clone(),
            Arc::new(InMemoryGraphCache::new()),
        );
        let polluted_results =
            block_on(polluted_index.retrieve_candidates(&request("releaseNotes")))
                .expect("polluted query");
        let baseline_contents: Vec<String> = baseline_results
            .iter()
            .map(|result| result.content.clone())
            .collect();
        let polluted_contents: Vec<String> = polluted_results
            .iter()
            .map(|result| result.content.clone())
            .collect();
        assert_eq!(
            baseline_contents, polluted_contents,
            "forged/null/foreign/missing stamps must contribute nothing to community summaries"
        );
        for contents in [&baseline_contents, &polluted_contents] {
            assert!(
                contents
                    .iter()
                    .any(|content| content.contains("releaseNotes")),
                "the ordinary non-code member keeps serving: {contents:?}"
            );
            for content in contents {
                for forbidden in [
                    "forgedParser",
                    "nullParser",
                    "foreignParser",
                    "missingParser",
                ] {
                    assert!(
                        !content.contains(forbidden),
                        "a stamped lineage-free code entity must never emit: {content}"
                    );
                }
            }
        }
        // A query DIRECTLY matching the forged name never returns it.
        let forged_query =
            block_on(polluted_index.retrieve_candidates(&request("forgedParser nullParser")))
                .expect("forged query");
        assert!(
            forged_query.is_empty(),
            "queries matching only stamped lineage-free code return nothing: {:?}",
            forged_query
                .iter()
                .map(|r| r.content.clone())
                .collect::<Vec<_>>()
        );
    }

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
