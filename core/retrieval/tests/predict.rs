//! Predictive-retrieval baseline: deterministic hint generation from agent state.

use engram_retrieval::{AgentState, PredictiveRetriever, RecentActivityPredictor};
use futures::executor::block_on;

#[test]
fn predicts_queries_and_targets_from_recent_activity() {
    let predictor = RecentActivityPredictor::new();
    let state = AgentState {
        task: Some("deploy payments service".to_owned()),
        recent_queries: vec!["payments outage".to_owned()],
        recent_target_ids: vec!["svc-payments".to_owned(), "svc-payments".to_owned()],
    };

    let hints = block_on(predictor.predict_context(&state)).expect("predict");

    // queries = tokenized recent_queries ∪ task terms, deduped + sorted.
    assert_eq!(
        hints.queries,
        vec![
            "deploy".to_owned(),
            "outage".to_owned(),
            "payments".to_owned(),
            "service".to_owned(),
        ]
    );
    // target_ids deduped + sorted.
    assert_eq!(hints.target_ids, vec!["svc-payments".to_owned()]);
}

#[test]
fn empty_state_yields_empty_hints() {
    let predictor = RecentActivityPredictor::new();
    let hints = block_on(predictor.predict_context(&AgentState::default())).expect("predict");
    assert!(hints.queries.is_empty());
    assert!(hints.target_ids.is_empty());
}

#[test]
fn prediction_is_deterministic() {
    let predictor = RecentActivityPredictor::new();
    let state = AgentState {
        task: Some("alpha beta".to_owned()),
        recent_queries: vec!["beta gamma".to_owned()],
        recent_target_ids: vec!["t2".to_owned(), "t1".to_owned()],
    };

    let first = block_on(predictor.predict_context(&state)).expect("predict");
    let second = block_on(predictor.predict_context(&state)).expect("predict");

    assert_eq!(first, second);
    assert_eq!(
        first.queries,
        vec!["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()]
    );
    assert_eq!(first.target_ids, vec!["t1".to_owned(), "t2".to_owned()]);
}

#[test]
fn whitespace_and_punctuation_only_inputs_yield_no_query_terms() {
    let predictor = RecentActivityPredictor::new();
    let state = AgentState {
        task: Some("   !!!   ".to_owned()),
        recent_queries: vec![String::new(), "   ".to_owned()],
        recent_target_ids: Vec::new(),
    };
    let hints = block_on(predictor.predict_context(&state)).expect("predict");
    assert!(
        hints.queries.is_empty(),
        "non-alphanumeric input yields no query terms"
    );
    assert!(hints.target_ids.is_empty());
}

#[test]
fn agent_state_round_trips_through_serde() {
    let state = AgentState {
        task: Some("ship it".to_owned()),
        recent_queries: vec!["query".to_owned()],
        recent_target_ids: vec!["t1".to_owned()],
    };
    let json = serde_json::to_string(&state).expect("serialize");
    let restored: AgentState = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(state, restored);
}
