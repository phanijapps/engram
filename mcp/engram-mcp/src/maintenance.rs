//! Graph-maintenance MCP tools (ADR-0027).
//!
//! Reversible, plan/apply graph repair: deterministic candidate detection,
//! dry-run plan building, transactional apply, and health aggregates. Read-only
//! by default; `apply_maintenance_plan` with `mode: "apply"` is the only
//! mutating path.

use engram_domain::*;
use engram_knowledge::CoreError;
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::internal;

fn bad(ctx: &str, e: impl std::fmt::Display) -> ToolError {
    internal(CoreError::Adapter {
        adapter: "engram-mcp".to_owned(),
        message: format!("{ctx}: {e}"),
    })
}

pub(crate) fn scope_from_args(app: &App, args: &Value) -> Result<Scope, ToolError> {
    // scan-reliability AC2: a missing/null `scope` defaults to the launch
    // scope (the fused-per-project workspace every other tool uses) instead
    // of a deserialization error.
    match args.get("scope") {
        Some(v) if !v.is_null() => {
            serde_json::from_value(v.clone()).map_err(|e| bad("invalid scope", e))
        }
        _ => Ok(app.scope.clone()),
    }
}

/// Clone `args` with a defaulted `scope` — for tools that deserialize a whole
/// request struct (plan build/apply) rather than reading `scope` directly.
fn args_with_default_scope(app: &App, args: &Value) -> Value {
    let mut v = args.clone();
    if v.get("scope").map_or(true, |s| s.is_null()) {
        if let Ok(scope) = serde_json::to_value(&app.scope) {
            v["scope"] = scope;
        }
    }
    v
}

/// `list_maintenance_candidates`: deterministic orphan/low-confidence/unsupported/
/// duplicate detection. No LLM.
pub fn list_maintenance_candidates(app: &App, args: &Value) -> Result<Value, ToolError> {
    let handle = app.provider.require_graph_maintenance().map_err(internal)?;
    let scope = scope_from_args(app, args)?;
    let graph_id = args["graphId"].as_str().map(Id::from);
    let policy: MaintenancePolicy = serde_json::from_value(
        args.get("policy")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    )
    .map_err(|e| bad("invalid policy", e))?;
    let result =
        block_on(handle.detect_candidates(&scope, graph_id.as_ref(), &policy)).map_err(internal)?;
    let body = serde_json::to_string(&result).map_err(|e| bad("serialize", e))?;
    Ok(protocol::text_content(body))
}

/// `build_maintenance_plan`: build a dry-run plan (fills before/after previews;
/// never mutates). Takes a `MaintenancePlanRequest`.
pub fn build_maintenance_plan(app: &App, args: &Value) -> Result<Value, ToolError> {
    let handle = app.provider.require_graph_maintenance().map_err(internal)?;
    let request: MaintenancePlanRequest =
        serde_json::from_value(args_with_default_scope(app, args))
            .map_err(|e| bad("invalid plan request", e))?;
    let result = block_on(handle.build_plan(request)).map_err(internal)?;
    let body = serde_json::to_string(&result).map_err(|e| bad("serialize", e))?;
    Ok(protocol::text_content(body))
}

/// `apply_maintenance_plan`: apply (or preview) a reviewed plan. `mode` is
/// `"preview"` (stages, no commit) or `"apply"` (commits in one transaction).
pub fn apply_maintenance_plan(app: &App, args: &Value) -> Result<Value, ToolError> {
    let handle = app.provider.require_graph_maintenance().map_err(internal)?;
    let plan: MaintenancePlan = serde_json::from_value(args_with_default_scope(app, &args["plan"]))
        .map_err(|e| bad("invalid plan", e))?;
    let mode: ApplyMode =
        serde_json::from_value(args["mode"].clone()).map_err(|e| bad("invalid mode", e))?;
    let result = block_on(handle.apply_plan(&plan, mode)).map_err(internal)?;
    let body = serde_json::to_string(&result).map_err(|e| bad("serialize", e))?;
    Ok(protocol::text_content(body))
}

/// `graph_health`: point-in-time candidate + archived counts per scope/graph.
pub fn graph_health(app: &App, args: &Value) -> Result<Value, ToolError> {
    let handle = app.provider.require_graph_maintenance().map_err(internal)?;
    let scope = scope_from_args(app, args)?;
    let graph_id = args["graphId"].as_str().map(Id::from);
    let result = block_on(handle.graph_health(&scope, graph_id.as_ref())).map_err(internal)?;
    let body = serde_json::to_string(&result).map_err(|e| bad("serialize", e))?;
    Ok(protocol::text_content(body))
}
