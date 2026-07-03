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
