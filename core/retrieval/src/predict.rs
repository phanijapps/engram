//! Predictive retrieval — proactive context hints derived from agent state.
//!
//! Deterministic baseline for the research's predictive-retrieval layer
//! (`docs/research/architecture-design-v2.md:511-524`): `predict_context` derives
//! retrieval hints from recent agent activity so the query router can proactively
//! load likely-relevant context before an explicit query. A model-assisted
//! predictor (expectation models, prediction-error / surprise signals, hierarchical
//! multi-level prediction) is deferred; this baseline is deterministic and
//! dependency-free.

use std::collections::BTreeSet;

use async_trait::async_trait;
use engram_runtime::CoreResult;
use serde::{Deserialize, Serialize};

/// Snapshot of what the agent is currently doing, used to predict likely-relevant
/// context. Carries the current task label and recent activity (explicit queries,
/// retrieved target ids).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub recent_queries: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub recent_target_ids: Vec<String>,
}

/// Proactive retrieval hints produced by prediction, consumed by the query
/// router alongside explicit queries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalHints {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub queries: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub target_ids: Vec<String>,
}

/// Derives proactive retrieval hints from agent state.
///
/// Implementations may use recent activity, expectation models, or prediction
/// errors. The contract is storage- and model-agnostic.
#[async_trait]
pub trait PredictiveRetriever: Send + Sync {
    /// Returns the retrieval hints predicted from the supplied agent state.
    async fn predict_context(&self, state: &AgentState) -> CoreResult<RetrievalHints>;
}

/// Deterministic baseline predictor.
///
/// Predicts that the agent will likely need what it recently needed: recent
/// queries plus the current task's terms become predicted queries, and recently
/// retrieved target ids are hinted as still-relevant. No model provider, clock,
/// or storage dependency.
#[derive(Debug, Clone, Default)]
pub struct RecentActivityPredictor;

impl RecentActivityPredictor {
    /// Creates a deterministic recent-activity predictor.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PredictiveRetriever for RecentActivityPredictor {
    async fn predict_context(&self, state: &AgentState) -> CoreResult<RetrievalHints> {
        let mut queries = BTreeSet::new();
        for query in &state.recent_queries {
            for term in tokenize(query) {
                queries.insert(term);
            }
        }
        if let Some(task) = &state.task {
            for term in tokenize(task) {
                queries.insert(term);
            }
        }
        let target_ids = state
            .recent_target_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        Ok(RetrievalHints {
            queries: queries.into_iter().collect(),
            target_ids: target_ids.into_iter().collect(),
        })
    }
}

/// Splits text into lowercase alphanumeric terms (mirrors the retrieval baseline's
/// `query_terms` tokenizer; no stopword filtering in the baseline).
fn tokenize(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| !character.is_alphanumeric())
        .filter_map(|term| {
            let term = term.trim().to_lowercase();
            (!term.is_empty()).then_some(term)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_state_yields_empty_hints() {
        let predictor = RecentActivityPredictor::new();
        let hints = predictor
            .predict_context(&AgentState::default())
            .await
            .unwrap();
        assert!(hints.queries.is_empty());
        assert!(hints.target_ids.is_empty());
    }

    #[tokio::test]
    async fn recent_queries_are_tokenized_into_predicted_queries() {
        let predictor = RecentActivityPredictor::new();
        let state = AgentState {
            recent_queries: vec!["auth login".to_owned(), "session-token".to_owned()],
            ..Default::default()
        };
        let hints = predictor.predict_context(&state).await.unwrap();
        // Tokenized (lowercased, split on non-alphanumeric), deduped via BTreeSet.
        assert!(hints.queries.contains(&"auth".to_owned()));
        assert!(hints.queries.contains(&"login".to_owned()));
        assert!(hints.queries.contains(&"session".to_owned()));
        assert!(hints.queries.contains(&"token".to_owned()));
        // No duplicates.
        assert_eq!(hints.queries.len(), 4);
        assert!(hints.target_ids.is_empty());
    }

    #[tokio::test]
    async fn task_terms_are_added_to_predicted_queries() {
        let predictor = RecentActivityPredictor::new();
        let state = AgentState {
            task: Some("Refactor the payment service".to_owned()),
            ..Default::default()
        };
        let hints = predictor.predict_context(&state).await.unwrap();
        assert!(hints.queries.contains(&"refactor".to_owned()));
        assert!(hints.queries.contains(&"payment".to_owned()));
        assert!(hints.queries.contains(&"service".to_owned()));
    }

    #[tokio::test]
    async fn recent_target_ids_pass_through_deduped() {
        let predictor = RecentActivityPredictor::new();
        let state = AgentState {
            recent_target_ids: vec!["e1".to_owned(), "e2".to_owned(), "e1".to_owned()],
            ..Default::default()
        };
        let hints = predictor.predict_context(&state).await.unwrap();
        assert_eq!(hints.target_ids, vec!["e1".to_owned(), "e2".to_owned()]);
        assert!(hints.queries.is_empty());
    }
}
