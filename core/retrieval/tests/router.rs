use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::*;
use engram_retrieval::{
    ReciprocalRankFusion, RetrievalCompositionInput, RetrievalIndex, RetrievalRoute,
    RetrievalRouteMode, RetrievalRouter, compose_context,
};
use engram_runtime::{CoreError, CoreResult};
use futures::executor::block_on;

struct StaticIndex {
    source: &'static str,
    target_id: &'static str,
    fail: bool,
}

#[async_trait]
impl RetrievalIndex for StaticIndex {
    async fn retrieve_candidates(
        &self,
        _request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        if self.fail {
            return Err(CoreError::Adapter {
                adapter: self.source.to_owned(),
                message: "fixture failure".to_owned(),
            });
        }
        Ok(vec![result(self.source, self.target_id)])
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-router"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "tenant-router".to_owned(),
        subject: None,
        workspace: Some("workspace-router".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn request(modes: Vec<RetrievalMode>) -> RetrievalRequest {
    RetrievalRequest {
        query: "router query".to_owned(),
        scope: scope(),
        requester: Requester {
            actor: actor(),
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        },
        modes,
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: Some(true),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "router_test".to_owned(),
        actor: actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("fixture".to_owned()),
    }
}

fn result(source: &str, target_id: &str) -> RetrievalResult {
    RetrievalResult {
        id: format!("{source}-{target_id}"),
        target_type: RetrievalTargetType::Memory,
        target_id: target_id.to_owned(),
        content: format!("{source} content"),
        score: RetrievalScore {
            total: 1.0,
            relevance: Some(1.0),
            recency: None,
            confidence: Some(1.0),
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: provenance(),
        policy: policy(),
        explanation: Some(RetrievalExplanation {
            reason: format!("{source} matched"),
            matched_cues: Vec::new(),
            matched_terms: vec![source.to_owned()],
            path: Vec::new(),
            source_summary: Some(source.to_owned()),
        }),
        fusion_trace: Some(FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: source.to_owned(),
            source_rank: Some(1),
            source_score: Some(1.0),
            score: None,
            rank: None,
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(1.0),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(1.0),
            discard_reason: None,
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}

fn route(source: &'static str, mode: RetrievalRouteMode) -> RetrievalRoute {
    RetrievalRoute::new(
        source,
        mode,
        Arc::new(StaticIndex {
            source,
            target_id: source,
            fail: false,
        }),
    )
}

#[test]
fn router_selects_all_research_modes_when_request_has_no_mode_filter() {
    let router = RetrievalRouter::new(vec![
        route("temporal", RetrievalRouteMode::Temporal),
        route("cue", RetrievalRouteMode::Cue),
        route("hierarchical", RetrievalRouteMode::Hierarchical),
        route("semantic", RetrievalRouteMode::Semantic),
        route("graph", RetrievalRouteMode::Graph),
        route("keyword", RetrievalRouteMode::Keyword),
        route("vector", RetrievalRouteMode::Vector),
    ]);

    let routed = block_on(router.retrieve(&request(Vec::new()))).expect("route candidates");
    let sources = routed
        .candidates
        .iter()
        .map(|result| result.fusion_trace.as_ref().unwrap().source.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        sources,
        vec![
            "temporal",
            "cue",
            "hierarchical",
            "semantic",
            "graph",
            "keyword",
            "vector"
        ]
    );
    assert!(routed.source_failures.is_empty());
}

#[test]
fn semantic_request_selects_semantic_and_vector_routes() {
    let router = RetrievalRouter::new(vec![
        route("semantic", RetrievalRouteMode::Semantic),
        route("vector", RetrievalRouteMode::Vector),
        route("graph", RetrievalRouteMode::Graph),
    ]);

    let routed = block_on(router.retrieve(&request(vec![RetrievalMode::Semantic])))
        .expect("route candidates");
    let sources = routed
        .candidates
        .iter()
        .map(|result| result.fusion_trace.as_ref().unwrap().source.as_str())
        .collect::<Vec<_>>();

    assert_eq!(sources, vec!["semantic", "vector"]);
}

#[test]
fn router_reports_unsupported_requested_modes_without_hiding_supported_results() {
    let router = RetrievalRouter::new(vec![route("graph", RetrievalRouteMode::Graph)]);

    let routed =
        block_on(router.retrieve(&request(vec![RetrievalMode::Graph, RetrievalMode::Cue])))
            .expect("route candidates");

    assert_eq!(routed.candidates.len(), 1);
    assert_eq!(routed.source_failures.len(), 1);
    assert_eq!(routed.source_failures[0].mode, Some(RetrievalMode::Cue));
    assert_eq!(routed.source_failures[0].reason, "unsupported_mode");
    assert!(routed.source_failures[0].degraded);
}

#[test]
fn router_turns_source_errors_into_degraded_failures() {
    let router = RetrievalRouter::new(vec![RetrievalRoute::new(
        "semantic",
        RetrievalRouteMode::Semantic,
        Arc::new(StaticIndex {
            source: "semantic",
            target_id: "semantic",
            fail: true,
        }),
    )]);

    let routed = block_on(router.retrieve(&request(vec![RetrievalMode::Semantic])))
        .expect("route candidates");

    assert!(routed.candidates.is_empty());
    assert_eq!(routed.source_failures[0].source, "semantic");
    assert_eq!(routed.source_failures[0].reason, "source_error");
    assert_eq!(
        routed.source_failures[0].severity,
        SourceFailureSeverity::Error
    );
}

#[test]
fn routed_candidates_compose_with_budget_omissions_and_failures() {
    let router = RetrievalRouter::new(vec![
        route("semantic", RetrievalRouteMode::Semantic),
        route("vector", RetrievalRouteMode::Vector),
    ]);
    let mut retrieval_request = request(vec![RetrievalMode::Semantic, RetrievalMode::Cue]);
    retrieval_request.limit = Some(1);

    let routed = block_on(router.retrieve(&retrieval_request)).expect("route candidates");
    let context = compose_context(RetrievalCompositionInput {
        request: &retrieval_request,
        fusion: &ReciprocalRankFusion::default(),
        reranker: None,
        candidates: routed.candidates,
        omitted: Vec::new(),
        source_failures: routed.source_failures,
        created_at: Utc::now(),
    })
    .expect("compose context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.omitted.len(), 1);
    assert_eq!(context.omitted[0].reason, OmittedReason::BudgetExceeded);
    assert_eq!(context.source_failures[0].reason, "unsupported_mode");
}
