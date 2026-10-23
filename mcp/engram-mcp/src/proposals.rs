//! Operator proposal tools (enhancements adoption, slice 2): submit, list,
//! approve, and reject inert pending writes over the `proposals` provider
//! handle.
//!
//! Proposals are NEVER recall targets; only a promoted memory becomes
//! recallable. Approval routes through the policy-checked memory write path.
//! The `operator` argument on approve/reject names the trusted reviewing
//! actor supplied by the host configuration — an agent-supplied identity in
//! a proposal payload never grants approval powers (the injected
//! `ProposalAuthorizer` decides).

use engram_domain::{
    Actor, ActorId, ActorKind, AllowedUse, DeleteMode, MemoryContent, MemoryKind, MemoryLink,
    OperatorProposalStatus, Policy, ProposalApproveRequest, ProposalListRequest,
    ProposalRejectRequest, ProposalSubmitRequest, Retention, Visibility,
};
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::{internal, req_str};

fn payload_from_args(
    args: &Value,
) -> Result<(MemoryKind, MemoryContent, Policy, Vec<MemoryLink>), ToolError> {
    let text = req_str(args, "text")?.to_owned();
    let kind = match args["kind"].as_str().unwrap_or("fact") {
        "fact" => MemoryKind::Fact,
        "preference" => MemoryKind::Preference,
        "episode" => MemoryKind::Episode,
        "observation" => MemoryKind::Observation,
        "artifact" => MemoryKind::Artifact,
        other => {
            return Err(crate::tools::invalid(format!(
                "unsupported proposal kind '{other}' (fact|preference|episode|observation|artifact)"
            )));
        }
    };
    let policy = Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    };
    Ok((
        kind,
        MemoryContent {
            text,
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        policy,
        Vec::new(),
    ))
}

fn actor_from(id: &str, display: Option<&str>) -> Actor {
    Actor {
        id: ActorId::from(id),
        kind: ActorKind::User,
        display_name: Some(display.unwrap_or(id).to_owned()),
        metadata: None,
    }
}

fn render(p: &engram_domain::OperatorProposal) -> String {
    let mut line = format!(
        "- [{}] {} status={} v{} by {}",
        p.id,
        truncate(&p.payload.content.text, 72),
        p.status.as_str(),
        p.version,
        p.submitted_by.id
    );
    if let Some(d) = &p.decision {
        line.push_str(&format!(
            " decided_by={}{}",
            d.reviewer.id,
            d.reason
                .as_deref()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default()
        ));
    }
    if let Some(m) = &p.promotion {
        line.push_str(&format!(" promoted_memory={}", m.memory_id));
    }
    line
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// `proposal_submit`: submit an inert pending memory write for operator review.
pub fn proposal_submit(app: &App, args: &Value) -> Result<Value, ToolError> {
    let (kind, content, policy, links) = payload_from_args(args)?;
    let submitted_by = actor_from(
        args["agent"].as_str().unwrap_or("mcp-agent"),
        args["agent_name"].as_str(),
    );
    let service = app.provider.require_proposals().map_err(internal)?;
    let result = block_on(service.submit(ProposalSubmitRequest {
        scope: app.scope.clone(),
        payload: engram_domain::ProposalMemoryWrite {
            kind,
            content,
            policy,
            links,
        },
        submitted_by,
        submission_key: args["submission_key"].as_str().map(str::to_owned),
    }))
    .map_err(internal)?;
    Ok(protocol::text_content(format!(
        "Proposal submitted for review (inert until approved; it is NEVER a recall target):\n{}",
        render(&result.proposal)
    )))
}

/// `proposal_list`: list queue records (optionally by status), paged.
pub fn proposal_list(app: &App, args: &Value) -> Result<Value, ToolError> {
    let status = match args["status"].as_str() {
        Some("pending") => Some(OperatorProposalStatus::Pending),
        Some("applying") => Some(OperatorProposalStatus::Applying),
        Some("approved") => Some(OperatorProposalStatus::Approved),
        Some("rejected") => Some(OperatorProposalStatus::Rejected),
        _ => None,
    };
    let limit = args["limit"].as_u64().unwrap_or(20).min(100) as u32;
    let after = args["after"]
        .as_str()
        .map(engram_domain::OperatorProposalId::from);
    let service = app.provider.require_proposals().map_err(internal)?;
    let result = block_on(service.list(ProposalListRequest {
        scope: app.scope.clone(),
        status,
        after,
        limit,
    }))
    .map_err(internal)?;
    if result.items.is_empty() {
        return Ok(protocol::text_content("No proposals."));
    }
    let mut body: Vec<String> = result.items.iter().map(render).collect();
    if let Some(cursor) = result.next_cursor {
        body.push(format!("(next page: after={cursor})"));
    }
    Ok(protocol::text_content(body.join("\n")))
}

/// `proposal_approve`: approve a pending proposal. Promotion routes through
/// the policy-checked memory write path; `applying` outcomes report the
/// in-progress state (retry-safe, deterministic idempotency key).
pub fn proposal_approve(app: &App, args: &Value) -> Result<Value, ToolError> {
    let id = req_str(args, "id")?;
    let operator = req_str(args, "operator")?;
    let expected_version = args["expected_version"].as_u64().unwrap_or(0);
    let service = app.provider.require_proposals().map_err(internal)?;
    let result = block_on(service.approve(ProposalApproveRequest {
        id: engram_domain::OperatorProposalId::from(id),
        scope: app.scope.clone(),
        expected_version,
        reviewer: actor_from(operator, None),
    }))
    .map_err(internal)?;
    let note = if result.proposal.status == OperatorProposalStatus::Applying {
        "Promotion IN PROGRESS: the memory write may have committed while the receipt finalize was interrupted; retry proposal_approve with the new version to finalize (safe — the write is idempotent)."
    } else {
        "Approved and promoted: the memory is now live through the normal policy-checked write path."
    };
    Ok(protocol::text_content(format!(
        "{note}\n{}",
        render(&result.proposal)
    )))
}

/// `proposal_reject`: reject a pending proposal (the payload stays inert and
/// is never promoted; edits require a fresh submission).
pub fn proposal_reject(app: &App, args: &Value) -> Result<Value, ToolError> {
    let id = req_str(args, "id")?;
    let operator = req_str(args, "operator")?;
    let expected_version = args["expected_version"].as_u64().unwrap_or(0);
    let reason = args["reason"].as_str().map(str::to_owned);
    let service = app.provider.require_proposals().map_err(internal)?;
    let result = block_on(service.reject(ProposalRejectRequest {
        id: engram_domain::OperatorProposalId::from(id),
        scope: app.scope.clone(),
        expected_version,
        reviewer: actor_from(operator, None),
        reason,
    }))
    .map_err(internal)?;
    Ok(protocol::text_content(format!(
        "Rejected (payload stays inert; no memory was written):\n{}",
        render(&result.proposal)
    )))
}
