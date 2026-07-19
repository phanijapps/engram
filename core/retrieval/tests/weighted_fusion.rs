use std::collections::BTreeMap;

use chrono::Utc;
use engram_domain::*;
use engram_retrieval::{RetrievalFusion, WeightedFusionConfig, WeightedRetrievalFusion};

#[test]
fn weighted_fusion_ranks_by_weighted_score() {
    let fusion = WeightedRetrievalFusion::new(
        WeightedFusionConfig::new(
            1.0,
            BTreeMap::from([("keyword".to_owned(), 1.0), ("vector".to_owned(), 2.0)]),
        )
        .expect("config"),
    );

    let results = fusion
        .fuse(
            &request(None),
            vec![
                result("keyword-1", "memory-a", 0.9, "keyword"),
                result("vector-1", "memory-b", 0.5, "vector"),
            ],
        )
        .expect("fuse");

    assert_eq!(
        results
            .iter()
            .map(|result| result.target_id.as_str())
            .collect::<Vec<_>>(),
        vec!["memory-b", "memory-a"]
    );
    assert_score(results[0].score.total, 1.0);
    assert_eq!(
        results[0]
            .fusion_trace
            .as_ref()
            .and_then(|trace| trace.fusion_strategy.as_ref()),
        Some(&FusionStrategy::WeightedSum)
    );
}

#[test]
fn weighted_fusion_collapses_duplicate_targets_with_trace() {
    let fusion = WeightedRetrievalFusion::default();

    let results = fusion
        .fuse(
            &request(None),
            vec![
                result("keyword-1", "memory-a", 0.7, "keyword"),
                result("vector-1", "memory-a", 0.6, "vector"),
                result("keyword-2", "memory-b", 0.4, "keyword"),
            ],
        )
        .expect("fuse");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].target_id, "memory-a");
    assert_eq!(results[0].content, "content for memory-a");
    assert_score(results[0].score.total, 1.3);
    let trace = results[0].fusion_trace.as_ref().expect("fusion trace");
    assert_eq!(trace.source, "keyword+vector");
    assert_eq!(trace.deduplicated_with, vec!["vector-1"]);
}

#[test]
fn weighted_fusion_applies_request_limit_after_ranking() {
    let fusion = WeightedRetrievalFusion::default();

    let results = fusion
        .fuse(
            &request(Some(1)),
            vec![
                result("low", "memory-low", 0.1, "keyword"),
                result("high", "memory-high", 0.9, "vector"),
            ],
        )
        .expect("fuse");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].target_id, "memory-high");
}

#[test]
fn weighted_fusion_rejects_invalid_weights() {
    assert!(WeightedFusionConfig::new(-1.0, BTreeMap::new()).is_err());
    assert!(
        WeightedFusionConfig::new(1.0, BTreeMap::from([("vector".to_owned(), f32::NAN)])).is_err()
    );
}

fn assert_score(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.0001,
        "expected score {expected}, got {actual}"
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
        target_type: RetrievalTargetType::Memory,
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
        explanation: Some(RetrievalExplanation {
            reason: source.to_owned(),
            matched_cues: Vec::new(),
            matched_terms: vec!["memory".to_owned()],
            path: Vec::new(),
            source_summary: None,
        }),
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
