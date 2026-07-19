use chrono::Utc;
use engram_domain::*;
use engram_retrieval::{
    DEFAULT_RRF_K, ReciprocalFusionConfig, ReciprocalRankFusion, RetrievalFusion,
};
use std::collections::BTreeMap;

#[test]
fn consensus_outranks_single_source() {
    // chunk-A appears at rank 1 in BOTH graph and vector; chunk-B is rank 1 in
    // graph only. RRF must rank A above B (cross-retriever consensus).
    let fused = ReciprocalRankFusion::default()
        .fuse(
            &request(None),
            vec![
                result("graph-1", "chunk-A", 0.0, "graph"),
                result("graph-2", "chunk-B", 0.0, "graph"),
                result("vector-1", "chunk-A", 0.0, "vector"),
            ],
        )
        .expect("fuse");
    let ids: Vec<&str> = fused.iter().map(|r| r.target_id.as_str()).collect();
    assert_eq!(ids, ["chunk-A", "chunk-B"], "consensus candidate leads");
}

#[test]
fn per_source_rank_not_global_index() {
    // Interleaved flat list: per-source rank must be used, not the global index,
    // so a rank-1 vector item ties/beats a rank-1 graph item and both beat rank-2.
    let fused = ReciprocalRankFusion::default()
        .fuse(
            &request(None),
            vec![
                result("graph-1", "G1", 0.0, "graph"),   // graph rank 1
                result("vector-1", "V1", 0.0, "vector"), // vector rank 1
                result("graph-2", "G2", 0.0, "graph"),   // graph rank 2
            ],
        )
        .expect("fuse");
    assert_eq!(fused.len(), 3);
    assert_eq!(fused.last().unwrap().target_id, "G2", "rank-2 item is last");
}

#[test]
fn stamps_reciprocal_strategy_trace() {
    let fused = ReciprocalRankFusion::default()
        .fuse(
            &request(None),
            vec![
                result("graph-1", "A", 0.0, "graph"),
                result("vector-1", "A", 0.0, "vector"),
            ],
        )
        .expect("fuse");
    let trace = fused[0].fusion_trace.as_ref().expect("trace");
    assert_eq!(
        trace.fusion_strategy,
        Some(FusionStrategy::ReciprocalRankFusion)
    );
    assert!(trace.fusion_score.unwrap_or(0.0) > 0.0);
    assert_eq!(trace.source, "graph+vector", "consensus source is multi");
}

#[test]
fn ignores_raw_scores_across_sources() {
    // graph item has score 0.99, vector item 0.01 — both rank 1 in their own
    // list. RRF must treat them equally (raw scores are irrelevant).
    let fused = ReciprocalRankFusion::default()
        .fuse(
            &request(None),
            vec![
                result("graph-1", "A", 0.99, "graph"),
                result("vector-1", "B", 0.01, "vector"),
            ],
        )
        .expect("fuse");
    let a = fused.iter().find(|r| r.target_id == "A").unwrap();
    let b = fused.iter().find(|r| r.target_id == "B").unwrap();
    assert!(
        (a.score.total - b.score.total).abs() < 1e-9,
        "equal per-source rank => equal RRF score regardless of raw scores"
    );
}

#[test]
fn empty_returns_empty() {
    let fused = ReciprocalRankFusion::default()
        .fuse(&request(None), Vec::new())
        .expect("fuse");
    assert!(fused.is_empty());
}

#[test]
fn respects_limit() {
    let fused = ReciprocalRankFusion::default()
        .fuse(
            &request(Some(2)),
            vec![
                result("g1", "A", 0.0, "graph"),
                result("g2", "B", 0.0, "graph"),
                result("g3", "C", 0.0, "graph"),
            ],
        )
        .expect("fuse");
    assert_eq!(fused.len(), 2);
}

#[test]
fn default_k_is_60() {
    assert_eq!(DEFAULT_RRF_K, 60);
    assert_eq!(ReciprocalRankFusion::default().k(), 60);
}

#[test]
fn weighted_rrf_biases_higher_weight_source() {
    // Two single-source candidates, both rank 1 in their own list. With graph
    // weight 3.0 vs vector 1.0, the graph item must outrank the vector item.
    let config = ReciprocalFusionConfig::new(60, 1.0, BTreeMap::from([("graph".to_owned(), 3.0)]))
        .expect("config");
    let fused = ReciprocalRankFusion::new(config)
        .fuse(
            &request(None),
            vec![
                result("graph-1", "G", 0.0, "graph"),
                result("vector-1", "V", 0.0, "vector"),
            ],
        )
        .expect("fuse");
    assert_eq!(fused[0].target_id, "G", "higher-weight source leads");
    assert_eq!(fused[1].target_id, "V");
    let g = fused.iter().find(|r| r.target_id == "G").unwrap();
    let v = fused.iter().find(|r| r.target_id == "V").unwrap();
    assert!(g.score.total > v.score.total, "graph score > vector score");
}

#[test]
fn default_config_is_pure_rrf() {
    // Default config (equal weights) must match pure RRF: two rank-1 items from
    // different sources get equal scores.
    let fused = ReciprocalRankFusion::default()
        .fuse(
            &request(None),
            vec![
                result("graph-1", "G", 0.0, "graph"),
                result("vector-1", "V", 0.0, "vector"),
            ],
        )
        .expect("fuse");
    let g = fused.iter().find(|r| r.target_id == "G").unwrap();
    let v = fused.iter().find(|r| r.target_id == "V").unwrap();
    assert!(
        (g.score.total - v.score.total).abs() < 1e-9,
        "default config = pure RRF, equal scores"
    );
}

#[test]
fn config_rejects_zero_k_and_negative_weight() {
    assert!(ReciprocalFusionConfig::new(0, 1.0, BTreeMap::new()).is_err());
    assert!(ReciprocalFusionConfig::new(60, -1.0, BTreeMap::new()).is_err());
}

#[test]
fn trace_reports_best_contributor_not_first_arriving() {
    // target X arrives from graph at rank 2 (first), then from vector at rank 1
    // (stronger). The trace must report the rank-1 contributor, not rank 2.
    let fused = ReciprocalRankFusion::default()
        .fuse(
            &request(None),
            vec![
                result("graph-1", "A", 0.0, "graph"),   // graph rank 1
                result("graph-2", "X", 0.0, "graph"),   // graph rank 2 (X first arrival)
                result("vector-1", "X", 0.0, "vector"), // vector rank 1 (X, stronger)
            ],
        )
        .expect("fuse");
    let x = fused
        .iter()
        .find(|r| r.target_id == "X")
        .expect("X present");
    let trace = x.fusion_trace.as_ref().expect("trace");
    assert_eq!(
        trace.source_rank,
        Some(1),
        "trace rank = best (rank-1), not first (rank-2)"
    );
    let expected_best = 1.0 / (60.0 + 1.0);
    assert!(
        (trace.source_score.unwrap_or(-1.0) - expected_best).abs() < 1e-9,
        "trace score = best contribution"
    );
}

fn request(limit: Option<u32>) -> RetrievalRequest {
    RetrievalRequest {
        query: "memory".to_owned(),
        scope: Scope {
            tenant: "tenant-demo".to_owned(),
            subject: None,
            workspace: Some("engram".to_owned()),
            session: None,
            environment: Some("test".to_owned()),
        },
        requester: Requester {
            actor: Actor {
                id: Id::from("actor-test"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        },
        modes: vec![RetrievalMode::Keyword, RetrievalMode::Semantic],
        filters: None,
        cues: Vec::new(),
        limit,
        budget: None,
        include_explanations: Some(true),
    }
}

fn result(id: &str, target_id: &str, score: f32, source: &str) -> RetrievalResult {
    RetrievalResult {
        id: id.to_owned(),
        target_type: RetrievalTargetType::Chunk,
        target_id: target_id.to_owned(),
        content: format!("content for {target_id}"),
        score: RetrievalScore {
            total: score,
            relevance: Some(score),
            recency: None,
            confidence: None,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: None,
        },
        provenance: Provenance {
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
        },
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Low),
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: Some(DeleteMode::Tombstone),
        },
        explanation: None,
        fusion_trace: Some(FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: source.to_owned(),
            source_rank: None,
            source_score: Some(score),
            score: None,
            rank: None,
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: None,
            rerank_strategy: None,
            rerank_score: None,
            discard_reason: None,
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}
