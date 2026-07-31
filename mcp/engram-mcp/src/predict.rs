//! Predictive-retrieval surface: expose the deterministic `RecentActivityPredictor`
//! so an agent can obtain proactive retrieval hints from its current state.
//!
//! The predictor lives in `engram-retrieval::predict`; it is dependency-free and
//! storage-agnostic, so this tool instantiates it directly (no provider handle).
//! An agent passes its `AgentState` (current task + recent queries/targets) and
//! gets back `RetrievalHints` (predicted queries + still-relevant target ids) to
//! feed into `recall` / `get_context`. Wiring the hints *into* the retrieve path
//! automatically is the deeper Phase-5 step (see the retrieval-completeness spec).

use engram_retrieval::{AgentState, PredictiveRetriever, RecentActivityPredictor};
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::internal;

/// Parse an optional `string[]` arg into a `Vec<String>`.
fn opt_str_array(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

/// Build an `AgentState` from the tool args. Pure + unit-testable (no `App`).
fn state_from_args(args: &Value) -> AgentState {
    AgentState {
        task: args
            .get("task")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned()),
        recent_queries: opt_str_array(args, "recent_queries"),
        recent_target_ids: opt_str_array(args, "recent_target_ids"),
    }
}

/// Derive the human-readable hints body from the tool args. Pure + unit-testable.
fn hints_body(args: &Value) -> Result<String, ToolError> {
    let state = state_from_args(args);
    let hints =
        block_on(RecentActivityPredictor::new().predict_context(&state)).map_err(internal)?;
    if hints.queries.is_empty() && hints.target_ids.is_empty() {
        return Ok(
            "No predictive hints (provide a task or recent_queries/recent_target_ids).".to_owned(),
        );
    }
    let queries = if hints.queries.is_empty() {
        "(none)".to_owned()
    } else {
        hints.queries.join(", ")
    };
    let target_ids = if hints.target_ids.is_empty() {
        "(none)".to_owned()
    } else {
        hints.target_ids.join(", ")
    };
    Ok(format!(
        "Predicted retrieval hints:\n  queries: {queries}\n  target_ids: {target_ids}\n\
         Feed these into recall / get_context to proactively load context.",
    ))
}

/// `predict_context`: derive proactive retrieval hints from the agent's current
/// state (task + recent activity).
pub fn predict_context(_app: &App, args: &Value) -> Result<Value, ToolError> {
    Ok(protocol::text_content(hints_body(args)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_args_yield_no_hints() {
        let body = hints_body(&serde_json::json!({})).unwrap();
        assert!(body.contains("No predictive hints"));
    }

    #[test]
    fn task_and_recent_queries_yield_hints() {
        let args = serde_json::json!({
            "task": "Refactor payment service",
            "recent_queries": ["auth login"],
            "recent_target_ids": ["e1", "e2"]
        });
        let body = hints_body(&args).unwrap();
        assert!(body.contains("refactor"));
        assert!(body.contains("payment"));
        assert!(body.contains("auth"));
        assert!(body.contains("e1"));
        assert!(body.contains("e2"));
    }

    #[test]
    fn target_ids_only_pass_through() {
        let args = serde_json::json!({ "recent_target_ids": ["e9"] });
        let body = hints_body(&args).unwrap();
        assert!(body.contains("e9"));
        assert!(body.contains("(none)") || !body.contains("queries: e9"));
    }
}
