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

/// `maintain op=outcomes`: the ΔP reporter — joins the outcome ledger with
/// retrieval stats to produce the first real measurement of whether memory
/// helps. Every line says ASSOCIATED, never causal (the contract rule).
pub fn outcome_report(app: &App, _args: &Value) -> Result<Value, ToolError> {
    let ledger = app.provider.require_outcomes().map_err(internal)?;
    let until = chrono::Utc::now();
    let since = until - chrono::Duration::hours(24 * 30);
    let outcomes = block_on(ledger.list_outcomes(&OutcomeWindow {
        scope: app.scope.clone(),
        since,
        until,
    }))
    .map_err(internal)?;

    if outcomes.is_empty() {
        return Ok(protocol::text_content(
            "No outcomes recorded. Call report_outcome after recall-informed tasks to start measuring ΔP.",
        ));
    }

    use std::collections::BTreeMap;
    let mut by_capability: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut total_with_recall = (0usize, 0usize);
    let mut total_without_recall = (0usize, 0usize);
    let mut interference_candidates: Vec<String> = Vec::new();

    for o in &outcomes {
        let cap = o
            .outcome
            .outcome
            .capability
            .as_deref()
            .unwrap_or("(unlabeled)");
        let entry = by_capability.entry(cap.to_owned()).or_insert((0, 0));
        entry.0 += 1;
        if o.outcome.outcome.success {
            entry.1 += 1;
        }
        if o.outcome.recalled_targets.is_empty() {
            total_without_recall.0 += 1;
            if o.outcome.outcome.success {
                total_without_recall.1 += 1;
            }
        } else {
            total_with_recall.0 += 1;
            if o.outcome.outcome.success {
                total_with_recall.1 += 1;
            }
        }
        if !o.outcome.recalled_targets.is_empty() && !o.outcome.outcome.success {
            interference_candidates.push(cap.to_owned());
        }
    }

    let rate_with = if total_with_recall.0 > 0 {
        total_with_recall.1 as f64 / total_with_recall.0 as f64
    } else {
        f64::NAN
    };
    let rate_without = if total_without_recall.0 > 0 {
        total_without_recall.1 as f64 / total_without_recall.0 as f64
    } else {
        f64::NAN
    };
    let dp = if rate_with.is_nan() || rate_without.is_nan() {
        f64::NAN
    } else {
        rate_with - rate_without
    };

    let mut lines = Vec::new();
    lines.push(format!(
        "ΔP Outcome Report ({} outcomes, last 30 days)",
        outcomes.len()
    ));
    lines.push(
        "  ASSOCIATED, not causal — causal credit requires matched interventions.".to_owned(),
    );
    lines.push(String::new());
    if !dp.is_nan() {
        lines.push(format!(
            "  ΔP (proxy): {:+.3}  (with recall: {:.1}% [{}/{}] | without: {:.1}% [{}/{}])",
            dp,
            rate_with * 100.0,
            total_with_recall.1,
            total_with_recall.0,
            rate_without * 100.0,
            total_without_recall.1,
            total_without_recall.0,
        ));
    } else {
        lines.push(format!(
            "  ΔP: insufficient data (with-recall: {}, without-recall: {})",
            total_with_recall.0, total_without_recall.0,
        ));
    }
    lines.push(String::new());
    lines.push("  By capability:".to_owned());
    for (cap, (total, success)) in &by_capability {
        let rate = *success as f64 / *total as f64 * 100.0;
        lines.push(format!("    {cap}: {success}/{total} ({rate:.0}%)"));
    }
    if !interference_candidates.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "  Interference candidates (failure despite recall): {}",
            interference_candidates.len()
        ));
        for cap in interference_candidates.iter().take(5) {
            lines.push(format!("    - {cap}"));
        }
    }
    lines.push(String::new());
    lines.push(format!(
        "  Total: {} outcomes. Report more via report_outcome after recall-informed tasks.",
        outcomes.len()
    ));

    Ok(protocol::text_content(lines.join("\n")))
}
