use chrono::Utc;
use engram_domain::*;
use engram_retrieval::{RetrievalCompositionInput, WeightedRetrievalFusion, compose_context};

#[test]
fn composer_returns_empty_context_for_empty_candidates() {
    let request = request(None, None);
    let created_at = Utc::now();
    let context = compose_context(RetrievalCompositionInput {
        request: &request,
        fusion: &WeightedRetrievalFusion::default(),
        candidates: Vec::new(),
        omitted: Vec::new(),
        source_failures: Vec::new(),
        created_at,
    })
    .expect("compose context");

    assert!(context.items.is_empty());
    assert!(context.omitted.is_empty());
    assert!(context.source_failures.is_empty());
    assert_eq!(context.created_at, created_at);
}

#[test]
fn composer_fuses_duplicates_before_final_budget_omissions() {
    let request = request(Some(1), None);
    let context = compose_context(RetrievalCompositionInput {
        request: &request,
        fusion: &WeightedRetrievalFusion::default(),
        candidates: vec![
            result("keyword-a", "memory-a", 0.7, "memory.keyword"),
            result("vector-a", "memory-a", 0.5, "vector.semantic"),
            result("keyword-b", "memory-b", 0.4, "memory.keyword"),
        ],
        omitted: Vec::new(),
        source_failures: Vec::new(),
        created_at: Utc::now(),
    })
    .expect("compose context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].target_id, "memory-a");
    assert_eq!(
        context.items[0]
            .fusion_trace
            .as_ref()
            .map(|trace| trace.deduplicated_with.clone()),
        Some(vec!["vector-a".to_owned()])
    );
    assert_eq!(
        context
            .omitted
            .iter()
            .map(|omitted| (&omitted.target_id, &omitted.reason))
            .collect::<Vec<_>>(),
        vec![(&"memory-b".to_owned(), &OmittedReason::BudgetExceeded)]
    );
}

#[test]
fn composer_uses_budget_limit_when_it_is_smaller_than_request_limit() {
    let request = request(
        Some(3),
        Some(ContextBudget {
            max_items: Some(1),
            max_tokens: None,
            max_bytes: None,
        }),
    );
    let context = compose_context(RetrievalCompositionInput {
        request: &request,
        fusion: &WeightedRetrievalFusion::default(),
        candidates: vec![
            result("high", "memory-high", 0.9, "memory.keyword"),
            result("low", "memory-low", 0.1, "memory.keyword"),
        ],
        omitted: vec![OmittedResult {
            target_type: RetrievalTargetType::Chunk,
            target_id: "chunk-denied".to_owned(),
            reason: OmittedReason::PolicyDenied,
        }],
        source_failures: Vec::new(),
        created_at: Utc::now(),
    })
    .expect("compose context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].target_id, "memory-high");
    assert_eq!(context.omitted.len(), 2);
    assert_eq!(context.omitted[0].target_id, "chunk-denied");
    assert_eq!(context.omitted[1].target_id, "memory-low");
    assert_eq!(context.omitted[1].reason, OmittedReason::BudgetExceeded);
}

#[test]
fn composer_preserves_degraded_source_failures() {
    let request = request(None, None);
    let source_failure = RetrievalSourceFailure {
        source: "retrieval_index.1".to_owned(),
        mode: Some(RetrievalMode::Semantic),
        severity: SourceFailureSeverity::Warning,
        reason: "external_retrieval_index_failed".to_owned(),
        message: Some("adapter failed".to_owned()),
        degraded: true,
    };
    let context = compose_context(RetrievalCompositionInput {
        request: &request,
        fusion: &WeightedRetrievalFusion::default(),
        candidates: vec![result("memory", "memory-a", 0.8, "memory.keyword")],
        omitted: Vec::new(),
        source_failures: vec![source_failure.clone()],
        created_at: Utc::now(),
    })
    .expect("compose context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.source_failures, vec![source_failure]);
}

fn request(limit: Option<u32>, budget: Option<ContextBudget>) -> RetrievalRequest {
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
        budget,
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
        explanation: None,
        fusion_trace: Some(FusionTrace {
            source: source.to_owned(),
            source_rank: None,
            source_score: Some(score),
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: None,
            rerank_strategy: None,
            rerank_score: None,
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}
