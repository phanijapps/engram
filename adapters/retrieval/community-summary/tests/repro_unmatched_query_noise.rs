//! REPRODUCTION: unmatched queries return scored noise from the
//! community-summary lane.
//!
//! Reported defect (live session, farm-expo graph): a query whose terms match
//! NOTHING in the knowledge graph still comes back with results from this
//! lane — every one stamped with a perfect 1.0 score — on nearly every query.
//! The result is background noise, not a search result.
//!
//! Source evidence (`src/index.rs`):
//!
//! 1. Communities are ranked by query-token overlap against a community
//!    summary string. When NO community matches, every score is 0.0 — but
//!    `ranked[0]` is still returned (all-zero tie-break = lowest community
//!    label), so the SAME community comes back whatever you ask.
//! 2. Every member of the winning community is emitted by `build_result`
//!    with a fabricated perfect score — `total`/`relevance`/`policy_fit` =
//!    1.0 and `source_score`/`fusion_score`/`rerank_score` = 1.0 — none of
//!    which has any relationship to the query. Downstream RRF preserves
//!    that fake `source_score` as the raw lane score, which is exactly the
//!    "score 1.0" observed live.
//!
//! Contract tests (`contract_*`) encode the DESIRED behavior — a result must
//! match the query before it can be ranked; graph expansion is only allowed
//! outward from actual matches. Green since the 2026-10-19 match-gate fix;
//! the regression guards above re-create the original defect's shape.
//! Witness tests (`defect_witness_*`) PIN today's broken behavior so the fix
//! can be proven to flip them; the fix must invert/delete them.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    Actor, ActorKind, AllowedUse, DeleteMode, EntityKind, EntityRef, FusionStrategy, FusionTrace,
    Id, KnowledgeEntity, KnowledgeRelationship, Policy, Provenance, Requester, RerankStrategy,
    Retention, RetrievalMode, RetrievalRequest, RetrievalResult, RetrievalScore, Scope,
    Sensitivity, Visibility,
};
use engram_retrieval::{ReciprocalRankFusion, RetrievalFusion, RetrievalIndex};
use engram_runtime::CoreResult;
use engram_store_associative_graph::GraphRelationshipSource;
use engram_store_community_summary::CommunitySummaryIndex;
use futures::executor::block_on;

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

fn scope() -> Scope {
    Scope {
        tenant: "t".to_owned(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
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
        provenance: Provenance {
            source: "test".to_owned(),
            actor: Actor {
                id: Id::from("test"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("test".to_owned()),
        },
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
        archived_at: None,
    }
}

fn rel(s: &str, o: &str) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(format!("r-{s}-{o}")),
        graph_id: None,
        subject: EntityRef {
            id: Some(Id::from(s)),
            kind: None,
            name: Some(s.to_owned()),
            aliases: Vec::new(),
        },
        predicate: "related_to".to_owned(),
        object: EntityRef {
            id: Some(Id::from(o)),
            kind: None,
            name: Some(o.to_owned()),
            aliases: Vec::new(),
        },
        scope: scope(),
        evidence: Vec::new(),
        confidence: None,
        provenance: Provenance {
            source: "test".to_owned(),
            actor: Actor {
                id: Id::from("test"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("test".to_owned()),
        },
        created_at: chrono::Utc::now(),
        updated_at: None,
        archived_at: None,
    }
}

fn request(query: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_owned(),
        scope: scope(),
        requester: Requester {
            actor: Actor {
                id: Id::from("test"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
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

/// Two disjoint communities, mirroring the live farm-expo store: an
/// agriculture cluster and a completely unrelated compiler cluster.
fn farm_expo_graph() -> StubSource {
    StubSource {
        entities: vec![
            ent("gh", "Greenhouse"),
            ent("di", "Drip Irrigation"),
            ent("ss", "Soil Moisture Sensor"),
            ent("ch", "Compost Heap"),
            ent("ra", "Rust Analyzer"),
            ent("bc", "Borrow Checker"),
            ent("ts", "Trait Solver"),
        ],
        relationships: vec![
            rel("gh", "di"),
            rel("di", "ss"),
            rel("ss", "ch"),
            rel("ch", "gh"),
            rel("ra", "bc"),
            rel("bc", "ts"),
            rel("ts", "ra"),
        ],
    }
}

/// Well-formed retrieval policy for synthetic hits in these tests.
fn entity_policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn describe(results: &[RetrievalResult]) -> String {
    results
        .iter()
        .map(|r| {
            format!(
                "{}(content={:?}, relevance={:?}, total={}, source_score={:?})",
                r.target_id,
                r.content,
                r.score.relevance,
                r.score.total,
                r.fusion_trace.as_ref().and_then(|t| t.source_score)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// Regression guards — FIXED 2026-10-19 (match gate + honest scores in the
// lane). These re-create the original live defect's exact shape.
// ---------------------------------------------------------------------------

/// The reported symptom, inverted: a query that shares no token with any
/// entity name or predicate returns NOTHING from this lane — no members, no
/// perfect scores.
#[test]
fn regression_unrelated_query_returns_nothing() {
    let index = CommunitySummaryIndex::new(Arc::new(farm_expo_graph()));

    let results =
        block_on(index.retrieve_candidates(&request("quantum chess grandmaster"))).unwrap();
    assert!(results.is_empty(), "got: {}", describe(&results));
}

/// Two entirely different unrelated queries both return nothing — the lane
/// is query-gated, not a constant background emitter.
#[test]
fn regression_unrelated_queries_return_nothing() {
    let index = CommunitySummaryIndex::new(Arc::new(farm_expo_graph()));

    let a = block_on(index.retrieve_candidates(&request("quantum chess grandmaster"))).unwrap();
    let b = block_on(index.retrieve_candidates(&request("medieval tapestry restoration"))).unwrap();
    assert!(a.is_empty() && b.is_empty());
}

/// Fusion consequence, inverted: with the lane correctly empty for an
/// unrelated query, fused recall contains ONLY the genuine match — no noise
/// outranking it, no fabricated raw 1.0 in any fusion trace.
#[test]
fn regression_fused_recall_has_no_zero_match_noise() {
    let index = CommunitySummaryIndex::new(Arc::new(farm_expo_graph()));

    let noise = block_on(index.retrieve_candidates(&request("quantum chess grandmaster"))).unwrap();
    assert!(
        noise.is_empty(),
        "lane must be empty for an unrelated query"
    );

    // A genuine lexical-style hit that actually matches the query, shaped
    // from a real entity so provenance/policy stay well-formed.
    let genuine = {
        let entity = ent("qc", "Quantum Chess Opening Theory");
        engram_domain::RetrievalResult {
            id: "lexical-chunk-qc".to_owned(),
            target_type: engram_domain::RetrievalTargetType::Chunk,
            target_id: "qc".to_owned(),
            content: "Quantum Chess Opening Theory".to_owned(),
            score: RetrievalScore {
                total: 0.8,
                relevance: Some(0.8),
                recency: None,
                confidence: None,
                cue_match: None,
                hierarchical_fit: None,
                policy_fit: Some(1.0),
            },
            provenance: entity.provenance.clone(),
            policy: entity_policy(),
            explanation: None,
            fusion_trace: Some(FusionTrace {
                query_id: None,
                vector_index: None,
                embedding_time_ms: None,
                search_time_ms: None,
                source: "lexical".to_owned(),
                source_rank: Some(1),
                source_score: Some(0.8),
                score: None,
                rank: None,
                fusion_strategy: Some(FusionStrategy::None),
                fusion_score: Some(0.8),
                rerank_strategy: Some(RerankStrategy::None),
                rerank_score: Some(0.8),
                discard_reason: None,
                deduplicated_with: Vec::new(),
            }),
            metadata: None,
        }
    };

    let fused = ReciprocalRankFusion::default()
        .fuse(&request("quantum chess grandmaster"), vec![genuine])
        .unwrap();

    assert_eq!(
        fused.len(),
        1,
        "only the genuine match survives: {}",
        describe(&fused)
    );
    assert_eq!(fused[0].target_id, "qc");
    assert_ne!(
        fused[0].fusion_trace.as_ref().and_then(|t| t.source_score),
        Some(1.0)
    );
}
// ---------------------------------------------------------------------------
// Contract — desired behavior. Green since the 2026-10-19 match-gate fix.
// ---------------------------------------------------------------------------

/// A query that matches nothing in the graph must produce NO candidates from
/// this lane. Graph lanes may only expand outward from actual matches; with
/// zero matches there is nothing to expand from.
#[test]
fn contract_unrelated_query_matches_nothing_returns_empty() {
    let index = CommunitySummaryIndex::new(Arc::new(farm_expo_graph()));

    let results =
        block_on(index.retrieve_candidates(&request("quantum chess grandmaster"))).unwrap();

    assert!(
        results.is_empty(),
        "non-matching query must return no results, got: {}",
        describe(&results)
    );
}

/// No result may claim a perfect relevance score it did not earn from an
/// actual query match. Even when a community DOES match, member scores must
/// reflect the match strength rather than a constant 1.0.
#[test]
fn contract_no_result_claims_unearned_perfect_relevance() {
    let index = CommunitySummaryIndex::new(Arc::new(farm_expo_graph()));

    // Weak match: a single query token overlaps the winning community's
    // summary; the other community matches nothing.
    let results = block_on(index.retrieve_candidates(&request("greenhouse"))).unwrap();

    let perfect: Vec<&str> = results
        .iter()
        .filter(|r| r.score.relevance == Some(1.0) || r.score.total == 1.0)
        .map(|r| r.target_id.as_str())
        .collect();
    assert!(
        perfect.is_empty(),
        "members must not carry fabricated 1.0 relevance (matched only one token), got: {}",
        describe(&results)
    );
}

/// AUDIT 2026-10-20 (search-quality): the match gate scored tokens against
/// the summary TEMPLATE string — "Community: ...; edges: ..." — so any query
/// containing "community", "edges", or the near-universal predicate words
/// ("related", "to") matched EVERY community and resurrected the
/// deterministic lowest-label noise for natural-language queries.
#[test]
fn audit_unrelated_query_with_stopwords_or_template_words_returns_nothing() {
    let index = CommunitySummaryIndex::new(Arc::new(farm_expo_graph()));
    for query in [
        "how to quantum grandmaster",     // "to" is inside related_to
        "learn community structure fast", // "community" is boilerplate
        "graph edges explanation",        // "edges" is boilerplate
        "related concepts overview",      // "related" from related_to
    ] {
        let results = block_on(index.retrieve_candidates(&request(query))).unwrap();
        assert!(
            results.is_empty(),
            "query {query:?} must not match via boilerplate/stopword tokens, got: {}",
            describe(&results)
        );
    }
}
