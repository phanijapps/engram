//! Outcome tools (F2): report attributed post-action results; read windows.
//!
//! The HOST reports outcomes (task execution and verification stay
//! host-side — engram is not a task executor). All surfaces speak
//! ASSOCIATION, never causation.

use engram_domain::{OutcomeWindow, ReportOutcomeRequest};
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::{internal, req_str};

/// `report_outcome`: attribute a task result to a recall's query id.
///
/// Attribution honesty (review P1-2): MCP submissions are CALLER-ASSERTED
/// telemetry — the verifier is recorded as the reporting AGENT, never as a
/// host/system checker (identity-looking payload data is not
/// authentication). Host-verified attribution is reserved for a trusted
/// embedding path.
pub fn report_outcome(app: &App, args: &Value) -> Result<Value, ToolError> {
    let query_id = req_str(args, "query_id")?;
    // Review P1-7: malformed telemetry is REJECTED, never defaulted to
    // failure — a missing or non-boolean `success` appends nothing.
    let success = args["success"]
        .as_bool()
        .ok_or_else(|| crate::tools::invalid("success is required and must be a boolean"))?;
    let capability = args["capability"].as_str().map(str::to_owned);
    let caller = args["verifier"].as_str().unwrap_or("mcp-agent");
    let verifier = engram_domain::Actor {
        id: engram_domain::ActorId::from(caller),
        kind: engram_domain::ActorKind::Agent,
        display_name: Some(caller.to_owned()),
        metadata: None,
    };
    let ledger = app.provider.require_outcomes().map_err(internal)?;
    let result = block_on(
        ledger.report_outcome(ReportOutcomeRequest {
            outcome: engram_domain::RecallOutcome {
                query_id: query_id.to_owned(),
                scope: app.scope.clone(),
                recalled_targets: args["recalled_targets"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
                omitted_targets: args["omitted_targets"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
                config_version: args["config_version"].as_str().map(str::to_owned),
                outcome: engram_domain::TaskOutcome {
                    success,
                    capability,
                    verifier,
                },
                occurred_at: chrono::Utc::now(),
            },
        }),
    )
    .map_err(internal)?;
    Ok(protocol::text_content(format!(
        "Outcome recorded (CALLER-ASSERTED telemetry — ASSOCIATED with the recall, never causal proof): id={} query={} success={}",
        result.recorded.id,
        result.recorded.outcome.query_id,
        result.recorded.outcome.outcome.success
    )))
}

/// `list_outcomes`: read a window of attributed outcomes (most recent first).
pub fn list_outcomes(app: &App, args: &Value) -> Result<Value, ToolError> {
    let ledger = app.provider.require_outcomes().map_err(internal)?;
    let hours = args["hours"].as_u64().unwrap_or(24).min(24 * 30) as i64;
    let until = chrono::Utc::now();
    let since = until - chrono::Duration::hours(hours);
    let outcomes = block_on(ledger.list_outcomes(&OutcomeWindow {
        scope: app.scope.clone(),
        since,
        until,
    }))
    .map_err(internal)?;
    if outcomes.is_empty() {
        return Ok(protocol::text_content(
            "No outcomes recorded in the window.",
        ));
    }
    let lines: Vec<String> = outcomes
        .iter()
        .map(|o| {
            format!(
                "- {} q={} success={}{} [{}]",
                o.id,
                o.outcome.query_id,
                o.outcome.outcome.success,
                o.outcome
                    .outcome
                    .capability
                    .as_deref()
                    .map(|c| format!(" capability={c}"))
                    .unwrap_or_default(),
                o.outcome.occurred_at
            )
        })
        .collect();
    Ok(protocol::text_content(format!(
        "Attributed outcomes (associated, not causal):\n{}",
        lines.join("\n")
    )))
}
