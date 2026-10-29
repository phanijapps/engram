//! TASK-014 focused proof (corrective r2): contributor-aware associative
//! invalidation on the REAL `AssociativeGraphIndex` (Personalized PageRank)
//! with a shared warm `GraphCache`.
//!
//! Eligibility enters through the engine-neutral `CodeEligibilityResolver`
//! port. This test drives it with a deterministic flip-fixture carrying
//! DURABLE LINEAGE (a typed double at that port boundary — the durable
//! SQLite resolver driving the same lane end to end is proven in
//! `engram-conformance`'s `bound_retrieval` target). The behavior under
//! proof is the adapter's own: the PPR universe, seeds and edges are
//! constrained BEFORE the walk (dangling/name-only endpoints drop outright —
//! finding 8), a warm snapshot materialized under a retired epoch is never
//! served again, and the request's repository narrowing applies before
//! traversal (finding 5).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::{
    Actor, ActorKind, EntityKind, EntityRef, Id, KnowledgeEntity, KnowledgeRelationship, Policy,
    Provenance, Requester, RetrievalRequest, RetrievalTargetType, Scope, Visibility,
};
use engram_retrieval::{
    BoundBinding, BoundCodeEligibility, CodeEligibilityResolver, DurableLineage,
    InMemoryGraphCache, RetrievalIndex,
};
use engram_runtime::CoreResult;
use engram_store_associative_graph::{AssociativeGraphIndex, GraphRelationshipSource, PprConfig};
use futures::executor::block_on;

// ---------- fixture ---------------------------------------------------------

/// Deterministic eligibility fixture at the resolver port: epoch 1 publishes
/// gen-1 of repo-a; the flip retires gen-1 and publishes gen-2 of repo-b.
/// DURABLE LINEAGE entries name which record ids are code.
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
                lineage: vec![
                    DurableLineage {
                        record_id: "entity-stale".to_owned(),
                        repo_id: "repo-a".to_owned(),
                        generation_id: "gen-1".to_owned(),
                        retired: false,
                        source_path: String::new(),
                        span: None,
                    },
                    DurableLineage {
                        record_id: "entity-stale-hub".to_owned(),
                        repo_id: "repo-a".to_owned(),
                        generation_id: "gen-1".to_owned(),
                        retired: false,
                        source_path: String::new(),
                        span: None,
                    },
                    DurableLineage {
                        record_id: "entity-eligible".to_owned(),
                        repo_id: "repo-b".to_owned(),
                        generation_id: "gen-2".to_owned(),
                        retired: false,
                        source_path: String::new(),
                        span: None,
                    },
                    DurableLineage {
                        record_id: "repo-b-crowd".to_owned(),
                        repo_id: "repo-b".to_owned(),
                        generation_id: "gen-2".to_owned(),
                        retired: false,
                        source_path: String::new(),
                        span: None,
                    },
                ],
            }),
        }
    }

    fn retire_gen1_publish_gen2(&self) {
        *self.current.lock().expect("flip lock") = BoundCodeEligibility {
            epoch: 2,
            bindings: vec![BoundBinding {
                repo_id: "repo-b".to_owned(),
                branch_ref: "refs/heads/develop".to_owned(),
                generation_id: "gen-2".to_owned(),
            }],
            lineage: vec![
                DurableLineage {
                    record_id: "entity-stale".to_owned(),
                    repo_id: "repo-a".to_owned(),
                    generation_id: "gen-1".to_owned(),
                    retired: false,
                    source_path: String::new(),
                    span: None,
                },
                DurableLineage {
                    record_id: "entity-stale-hub".to_owned(),
                    repo_id: "repo-a".to_owned(),
                    generation_id: "gen-1".to_owned(),
                    retired: false,
                    source_path: String::new(),
                    span: None,
                },
                DurableLineage {
                    record_id: "entity-eligible".to_owned(),
                    repo_id: "repo-b".to_owned(),
                    generation_id: "gen-2".to_owned(),
                    retired: false,
                    source_path: String::new(),
                    span: None,
                },
                DurableLineage {
                    record_id: "repo-b-crowd".to_owned(),
                    repo_id: "repo-b".to_owned(),
                    generation_id: "gen-2".to_owned(),
                    retired: false,
                    source_path: String::new(),
                    span: None,
                },
            ],
        };
    }
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

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        ..policy_rest()
    }
}

fn policy_rest() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: engram_domain::Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![engram_domain::AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn entity(id: &str, name: &str, kind: EntityKind) -> KnowledgeEntity {
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
        // Corrective r2 (finding 4): metadata is NOT authority — lineage is.
        // These entities deliberately carry NO stamps; code identity comes
        // from the fixture's durable lineage.
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

fn ref_without_id(name: &str) -> EntityRef {
    EntityRef {
        id: None,
        kind: None,
        name: Some(name.to_owned()),
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

/// A name-only edge (no object id) — the dangling-endpoint hazard (finding 8).
fn name_only_edge(id: &str, subject: &str, object_name: &str) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(id),
        graph_id: None,
        predicate: "calls".to_owned(),
        subject: ref_of(subject),
        object: ref_without_id(object_name),
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

fn request_narrowed(query: &str, keys: &[&str]) -> RetrievalRequest {
    let mut request = request(query);
    request.filters = Some(empty_filter_with_keys(
        keys.iter().map(|key| key.to_string()).collect(),
    ));
    request
}

fn empty_filter_with_keys(keys: Vec<String>) -> engram_domain::QueryFilter {
    engram_domain::QueryFilter {
        memory_kinds: Vec::new(),
        source_kinds: Vec::new(),
        chunk_kinds: Vec::new(),
        concept_ids: Vec::new(),
        entity_ids: Vec::new(),
        since: None,
        until: None,
        code_index_keys: Some(keys),
        min_confidence: None,
        include_archived: None,
        as_of: None,
        include_code_artifacts: None,
    }
}

/// The store KEEPS serving every record; only eligibility flips.

/// Like [`entity`] but carrying metadata stamps — FORGED/null/foreign
/// variations used to prove stamps are never classification authority.
fn stamped_entity(
    id: &str,
    name: &str,
    kind: EntityKind,
    generation_stamp: Option<serde_json::Value>,
    repo_stamp: Option<serde_json::Value>,
) -> KnowledgeEntity {
    let mut record = entity(id, name, kind);
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

fn graph() -> StubSource {
    StubSource {
        entities: vec![
            entity("entity-stale", "staleFn", EntityKind::Function),
            entity("entity-stale-hub", "staleHub", EntityKind::Struct),
            entity("entity-eligible", "eligibleFn", EntityKind::Function),
            entity("repo-b-crowd", "crowdFn", EntityKind::Function),
            // Non-code record: never in lineage, always eligible.
            entity("entity-shared", "sharedDoc", EntityKind::Concept),
        ],
        relationships: vec![
            edge("edge-1", "entity-stale", "entity-stale-hub", "calls"),
            edge("edge-2", "entity-stale", "entity-shared", "mentions"),
            edge("edge-3", "entity-eligible", "entity-shared", "mentions"),
            edge("edge-4", "repo-b-crowd", "entity-eligible", "calls"),
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

    /// Corrective r2 (finding 4): FORGED, null, foreign and missing metadata
    /// stamps on LINEAGE-FREE code entities never make them eligible through
    /// the REAL associative lane — they never seed or rank — while the
    /// ordinary non-code counterpart keeps serving.
    #[test]
    fn forged_and_missing_stamps_never_seed_or_rank() {
        let eligibility = Arc::new(FlipEligibility::gen1());
        eligibility.retire_gen1_publish_gen2();
        // The eligible universe plus four stamped lineage-free Functions
        // wired AHEAD of the eligible seed so an unsound classifier would
        // rank them first.
        let mut entities = graph().entities;
        let mut relationships = graph().relationships;
        entities.push(stamped_entity(
            "forged-fn",
            "forgedParser",
            EntityKind::Function,
            Some(serde_json::Value::String("gen-2".to_owned())),
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
        for id in [
            "forged-fn",
            "null-stamp-fn",
            "foreign-stamp-fn",
            "missing-stamp-fn",
        ] {
            relationships.push(edge(&format!("stamp-{id}"), id, "entity-eligible", "calls"));
        }
        let index = bound_index(
            Arc::new(StubSource {
                entities,
                relationships,
            }),
            eligibility,
            Arc::new(InMemoryGraphCache::new()),
        );

        // A query matching only the stamped names returns nothing.
        let forged = block_on(index.retrieve_candidates(&request(
            "forgedParser nullParser foreignParser missingParser",
        )))
        .expect("forged query");
        assert!(
            forged.is_empty(),
            "stamped lineage-free code never seeds: {:?}",
            forged
                .iter()
                .map(|r| r.target_id.clone())
                .collect::<Vec<_>>()
        );

        // The eligible query still serves the eligible + ordinary universe
        // and never leaks a stamped record through traversal.
        let eligible =
            block_on(index.retrieve_candidates(&request("eligibleFn"))).expect("eligible query");
        let ids: Vec<String> = eligible.iter().map(|r| r.target_id.clone()).collect();
        assert!(
            ids.contains(&"entity-eligible".to_owned()),
            "eligible code ranks: {ids:?}"
        );
        assert!(
            ids.contains(&"entity-shared".to_owned()),
            "the ordinary record keeps serving: {ids:?}"
        );
        for forbidden in [
            "forged-fn",
            "null-stamp-fn",
            "foreign-stamp-fn",
            "missing-stamp-fn",
        ] {
            assert!(
                !ids.iter().any(|id| id == forbidden),
                "a stamped lineage-free record must never rank or traverse: {ids:?}"
            );
        }
    }

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

        eligibility.retire_gen1_publish_gen2();
        let after =
            block_on(index.retrieve_candidates(&request("staleFn"))).expect("post-retire query");
        let ids: Vec<String> = after.iter().map(|r| r.target_id.clone()).collect();
        assert!(
            !ids.iter().any(|id| id.contains("stale")),
            "a retired generation must never seed, rank or traverse after epoch bump: {ids:?}"
        );

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
    /// surviving edge — recomputed from surviving inputs, never dropped
    /// wholesale and never bridging through the stale edge.
    #[test]
    fn mixed_lineage_recompute() {
        let eligibility = Arc::new(FlipEligibility::gen1());
        eligibility.retire_gen1_publish_gen2();
        let index = bound_index(
            Arc::new(graph()),
            eligibility,
            Arc::new(InMemoryGraphCache::new()),
        );

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

    /// Corrective r2 (finding 8): dangling/name-only edges never surface
    /// unauthorized nodes nor consume traversal budget — an eligible node's
    /// edge to a NAME-ONLY unbound symbol is dropped before the walk, while
    /// the valid authorized edge still traverses under a tight result limit.
    #[test]
    fn dangling_name_only_endpoints_never_traverse_or_consume_budget() {
        let eligibility = Arc::new(FlipEligibility::gen1());
        eligibility.retire_gen1_publish_gen2();
        let mut source = graph();
        // Dangling edges AHEAD of the valid edge (they would occupy the
        // traversal budget if endpoint authorization were absent).
        source.relationships.insert(
            0,
            name_only_edge("dangling-1", "entity-eligible", "ghostSymbol"),
        );
        source.relationships.insert(
            1,
            edge("dangling-2", "entity-eligible", "entity-stale", "calls"),
        );
        let index = bound_index(
            Arc::new(source),
            eligibility,
            Arc::new(InMemoryGraphCache::new()),
        );
        let mut tight = request("eligibleFn");
        tight.limit = Some(2);
        let results = block_on(index.retrieve_candidates(&tight)).expect("tight budget query");
        let ids: Vec<String> = results.iter().map(|r| r.target_id.clone()).collect();
        assert!(
            !ids.iter().any(|id| id.contains("ghost")),
            "a name-only endpoint must never surface as a node: {ids:?}"
        );
        assert!(
            !ids.iter().any(|id| id.contains("stale")),
            "an ineligible endpoint must never traverse: {ids:?}"
        );
        assert!(
            ids.contains(&"entity-shared".to_owned()),
            "the authorized edge must still traverse and fill the budget: {ids:?}"
        );
    }

    /// Corrective r2 (finding 5): the request's repository narrowing applies
    /// to the derived lane BEFORE the walk — with repo-b@refs/heads/develop
    /// requested, repo-b's crowd node cannot crowd out… (repo-b IS selected
    /// here); the inverse: narrowing to a WRONG ref selects nothing.
    #[test]
    fn requested_repository_narrowing_applies_before_the_walk() {
        let eligibility = Arc::new(FlipEligibility::gen1());
        eligibility.retire_gen1_publish_gen2();
        let index = bound_index(
            Arc::new(graph()),
            eligibility,
            Arc::new(InMemoryGraphCache::new()),
        );

        // Correct qualified key: repo-b's published develop binding serves.
        let selected = block_on(index.retrieve_candidates(&request_narrowed(
            "eligibleFn",
            &["repo-b@refs/heads/develop"],
        )))
        .expect("narrowed query");
        assert!(
            selected
                .iter()
                .any(|result| result.target_id == "entity-eligible"),
            "the correctly requested repository must serve: {:?}",
            selected
                .iter()
                .map(|r| r.target_id.clone())
                .collect::<Vec<_>>()
        );

        // Wrong ref for repo-b (published on develop, requested on main):
        // NOTHING is selected — the wrong-ref narrowing excludes repo-b's
        // code entirely (finding 9 semantics applied in the derived lane).
        let wrong_ref = block_on(
            index.retrieve_candidates(&request_narrowed("eligibleFn", &["repo-b@refs/heads/main"])),
        )
        .expect("wrong-ref query");
        assert!(
            wrong_ref
                .iter()
                .all(|result| result.target_type != RetrievalTargetType::Entity
                    || result.target_id == "entity-shared"),
            "a wrong-ref narrowing must exclude that repository's code: {:?}",
            wrong_ref
                .iter()
                .map(|r| r.target_id.clone())
                .collect::<Vec<_>>()
        );
    }
}
