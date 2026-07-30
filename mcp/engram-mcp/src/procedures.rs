//! Procedure tools (RFC-0016 Layer 6): assert, list, and account for replayable
//! runbooks over the `procedures` provider handle.
//!
//! Procedures are operational memory — a named sequence of steps to apply in a
//! recurring situation, with success/failure counters the caller bumps as the
//! procedure succeeds or fails. The server records outcomes; it does not decide
//! when a procedure applies.

use chrono::Utc;
use engram_domain::{Procedure, ProcedureId};
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::{internal, policy, provenance, req_str};

/// `procedure_put`: assert/upsert a replayable runbook (steps + optional trigger).
pub fn procedure_put(app: &App, args: &Value) -> Result<Value, ToolError> {
    let name = req_str(args, "name")?;
    let steps: Vec<String> = args["steps"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let trigger = args["trigger"].as_str().map(str::to_owned);
    let now = Utc::now();
    let procedure = Procedure {
        id: ProcedureId::from(name),
        scope: app.scope.clone(),
        name: name.to_owned(),
        steps,
        trigger,
        success_count: 0,
        failure_count: 0,
        provenance: provenance("mcp-procedure-put"),
        policy: policy(),
        created_at: now,
        updated_at: None,
        metadata: None,
    };
    let repo = app.provider.require_procedures().map_err(internal)?;
    let stored = block_on(repo.upsert_procedure(procedure)).map_err(internal)?;
    Ok(protocol::text_content(format!(
        "Procedure stored: '{}' ({} step(s){}) [id {}]",
        stored.name,
        stored.steps.len(),
        stored
            .trigger
            .as_deref()
            .map(|t| format!(", trigger '{t}'"))
            .unwrap_or_default(),
        stored.id
    )))
}

/// `procedure_list`: list procedures in the project scope.
pub fn procedure_list(app: &App, _args: &Value) -> Result<Value, ToolError> {
    let repo = app.provider.require_procedures().map_err(internal)?;
    let procedures = block_on(repo.list_procedures(&app.scope)).map_err(internal)?;
    let body = if procedures.is_empty() {
        "No procedures.".to_owned()
    } else {
        procedures
            .iter()
            .map(|p| {
                format!(
                    "- {} ({} step(s), {}✓ {}✗) [id {}]",
                    p.name,
                    p.steps.len(),
                    p.success_count,
                    p.failure_count,
                    p.id
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(protocol::text_content(body))
}

/// `procedure_increment`: bump the success or failure counter for a procedure.
pub fn procedure_increment(app: &App, args: &Value) -> Result<Value, ToolError> {
    let id = req_str(args, "id")?;
    let outcome = req_str(args, "outcome")?;
    let repo = app.provider.require_procedures().map_err(internal)?;
    let updated = match outcome {
        "success" => block_on(repo.increment_success(&ProcedureId::from(id), &app.scope)),
        "failure" => block_on(repo.increment_failure(&ProcedureId::from(id), &app.scope)),
        other => {
            return Err(ToolError::new(
                -32602,
                format!("unknown outcome: {other}; expected success|failure"),
            ));
        }
    }
    .map_err(internal)?;
    Ok(protocol::text_content(format!(
        "Procedure '{}' {outcome} — now {}✓ {}✗",
        updated.name, updated.success_count, updated.failure_count
    )))
}
