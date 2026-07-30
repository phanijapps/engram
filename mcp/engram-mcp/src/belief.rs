//! Belief-surface tools (RFC-0016 P2, Layer 5): assert, query, retract, and list
//! stale beliefs over the `beliefs` provider handle.
//!
//! Beliefs are the system's current stance over evidence — bi-temporal and
//! lifecycle-managed — the synthesized layer above raw facts and the knowledge
//! graph. Reads are query-based (the repository exposes no `list_beliefs`); use
//! `belief_get` for "what do we believe about X?" and `belief_stale_list` for
//! beliefs flagged for review.

use chrono::Utc;
use engram_belief::BeliefQuery;
use engram_domain::{Belief, BeliefStatus, BeliefSubject, Contradiction, Id};
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::{internal, policy, provenance, req_str};

/// Parse an optional RFC3339 `as_of`; default to now.
fn parse_as_of(args: &Value) -> Result<chrono::DateTime<Utc>, ToolError> {
    match args["as_of"].as_str() {
        None => Ok(Utc::now()),
        Some(s) => chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| ToolError::new(-32602, format!("invalid as_of (RFC3339): {e}"))),
    }
}

/// `belief_get`: the live belief for a subject (valid at `as_of`, default now).
pub fn belief_get(app: &App, args: &Value) -> Result<Value, ToolError> {
    let subject = req_str(args, "subject")?;
    let as_of = parse_as_of(args)?;
    let repo = app.provider.require_beliefs().map_err(internal)?;
    let query = BeliefQuery::live_subject(app.scope.clone(), subject, as_of);
    let belief = block_on(repo.get_belief(query)).map_err(internal)?;
    let body = match belief {
        Some(b) => format!(
            "Belief on '{}': {}\n  status: {:?} | confidence: {:.2} | valid: {:?}..{:?}\n  id: {}",
            b.subject.key, b.content, b.status, b.confidence, b.valid_from, b.valid_until, b.id
        ),
        None => format!("No live belief found for '{subject}'."),
    };
    Ok(protocol::text_content(body))
}

/// `belief_put`: assert/upsert a manual belief (a new valid-time version).
pub fn belief_put(app: &App, args: &Value) -> Result<Value, ToolError> {
    let subject = req_str(args, "subject")?;
    let statement = req_str(args, "statement")?;
    let confidence = args["confidence"].as_f64().unwrap_or(0.8).clamp(0.0, 1.0) as f32;
    let now = Utc::now();
    let belief = Belief {
        id: Id::from(format!("belief-{subject}-{}", now.timestamp())),
        scope: app.scope.clone(),
        subject: BeliefSubject {
            key: subject.to_owned(),
            entity_ref: None,
            concept_ref: None,
            aliases: Vec::new(),
        },
        content: statement.to_owned(),
        status: BeliefStatus::Active,
        confidence,
        sources: Vec::new(),
        valid_from: Some(now),
        valid_until: None,
        superseded_by: None,
        stale: None,
        synthesizer: None,
        reasoning: None,
        embedding_refs: Vec::new(),
        policy: policy(),
        provenance: provenance("mcp-belief-put"),
        created_at: now,
        updated_at: None,
        metadata: None,
    };
    let repo = app.provider.require_beliefs().map_err(internal)?;
    let stored = block_on(repo.upsert_belief(belief)).map_err(internal)?;
    Ok(protocol::text_content(format!(
        "Belief stored: '{}' = {} [{:?}, conf {:.2}, id {}]",
        stored.subject.key, stored.content, stored.status, stored.confidence, stored.id
    )))
}

/// `belief_retract`: close a belief's valid interval (retract by id).
pub fn belief_retract(app: &App, args: &Value) -> Result<Value, ToolError> {
    let id = req_str(args, "id")?;
    let now = Utc::now();
    let repo = app.provider.require_beliefs().map_err(internal)?;
    let retracted =
        block_on(repo.retract_belief(&Id::from(id), &app.scope, now)).map_err(internal)?;
    Ok(protocol::text_content(format!(
        "Belief {id} retracted [{:?}].",
        retracted.status
    )))
}

/// `belief_stale_list`: beliefs flagged stale in the project scope.
pub fn belief_stale_list(app: &App, _args: &Value) -> Result<Value, ToolError> {
    let repo = app.provider.require_beliefs().map_err(internal)?;
    let stale = block_on(repo.list_stale(&app.scope)).map_err(internal)?;
    let body = if stale.is_empty() {
        "No stale beliefs.".to_owned()
    } else {
        stale
            .iter()
            .map(|b| {
                format!(
                    "- {} ({}): {} [conf {:.2}]",
                    b.id, b.subject.key, b.content, b.confidence
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(protocol::text_content(body))
}

/// `contradiction_list`: lists open contradiction review records in the scope.
pub fn contradiction_list(app: &App, _args: &Value) -> Result<Value, ToolError> {
    let repo = app.provider.require_beliefs().map_err(internal)?;
    let contradictions = block_on(repo.list_contradictions(&app.scope)).map_err(internal)?;
    let body = if contradictions.is_empty() {
        "No contradictions.".to_owned()
    } else {
        contradictions
            .iter()
            .map(|c| {
                format!(
                    "- {:?}: {:?} [{:?}] {:?} | {}",
                    c.kind, c.status, c.severity, c.reasoning, c.id
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(protocol::text_content(body))
}
