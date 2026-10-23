//! Operator proposal queue (wicked-estate adoption, slice 2).
//!
//! Inert, reviewable pending writes. A proposal is NEVER a recall target:
//! only a successfully promoted memory becomes recallable through the normal
//! memory write path. Approval routes through `MemoryService::write_memory`
//! — and therefore the policy authorizer — never around it. There is no
//! cross-store transaction: a memory committed while the proposal finalize
//! fails leaves the proposal visibly `applying` (in-progress/recovery), never
//! silently rejected or rolled back.
//!
//! First accepted slice: **memory-write proposals only** (typed payload
//! derived from `WriteMemoryRequest` fields). Knowledge-write proposals
//! require a separately reviewed policy-checked knowledge approval route.

use serde::{Deserialize, Serialize};

use crate::{Actor, Id, MemoryContent, MemoryId, MemoryKind, MemoryLink, Policy, Scope, Timestamp};

pub type OperatorProposalId = Id;

/// Queue lifecycle. Legal transitions:
/// `pending → applying` (approval claim, CAS), `applying → approved`
/// (promotion receipt persisted), `pending → rejected` (operator decision).
/// `applying` is the visible in-progress/recovery state: a memory may have
/// committed while the receipt finalize failed; a retry re-runs the same
/// idempotent write and finalizes. Terminal states never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorProposalStatus {
    Pending,
    Applying,
    Approved,
    Rejected,
}

impl OperatorProposalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            OperatorProposalStatus::Pending => "pending",
            OperatorProposalStatus::Applying => "applying",
            OperatorProposalStatus::Approved => "approved",
            OperatorProposalStatus::Rejected => "rejected",
        }
    }

    pub fn can_transition_to(self, next: OperatorProposalStatus) -> bool {
        use OperatorProposalStatus::*;
        matches!(
            (self, next),
            (Pending, Applying) | (Applying, Approved) | (Pending, Rejected)
        )
    }
}

/// The proposed memory intent — a typed subset of `WriteMemoryRequest`.
/// The payload cannot set status, reviewer, memory id, or permissions;
/// those are owned by the queue and the trusted approving host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalMemoryWrite {
    pub kind: MemoryKind,
    pub content: MemoryContent,
    pub policy: Policy,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub links: Vec<MemoryLink>,
}

/// Operator decision record (approve or reject).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDecision {
    pub reviewer: Actor,
    pub decided_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Receipt for a promoted proposal: the memory the approval created, the
/// lifecycle event, and the deterministic idempotency key that makes
/// approval retries replay-safe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalPromotion {
    pub memory_id: MemoryId,
    pub event_id: Id,
    pub idempotency_key: String,
    pub applied_at: Timestamp,
}

/// One queue record. Governance of the QUEUE record is distinct from the
/// governance of the proposed memory; scope is immutable after submit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatorProposal {
    pub id: OperatorProposalId,
    pub scope: Scope,
    pub payload: ProposalMemoryWrite,
    pub status: OperatorProposalStatus,
    /// Submitting actor (evidence/on-behalf-of attribution only — never
    /// authorization).
    pub submitted_by: Actor,
    /// Deterministic fingerprint of the immutable payload (version pin).
    pub payload_fingerprint: String,
    /// Optimistic-concurrency version; every state transition bumps it.
    pub version: u64,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<ProposalDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion: Option<ProposalPromotion>,
}

impl OperatorProposal {
    /// The deterministic memory idempotency key for this proposal's
    /// promotion: one proposal promotes at most one memory, and approval
    /// retries replay the same write (never a shared batch key).
    pub fn promotion_idempotency_key(&self) -> String {
        format!("proposal-promotion-{}", self.id)
    }
}

// ── Operations ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalSubmitRequest {
    pub scope: Scope,
    pub payload: ProposalMemoryWrite,
    pub submitted_by: Actor,
    /// Optional scope-bound idempotency key: resubmitting the same key
    /// returns the existing queue record instead of duplicating it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submission_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalSubmitResult {
    pub proposal: OperatorProposal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalListRequest {
    pub scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<OperatorProposalStatus>,
    /// Opaque cursor (the last item's id); `None` starts from the beginning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<OperatorProposalId>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalListResult {
    pub items: Vec<OperatorProposal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<OperatorProposalId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalApproveRequest {
    pub id: OperatorProposalId,
    pub scope: Scope,
    /// Expected current version (optimistic concurrency).
    pub expected_version: u64,
    /// Trusted approving actor supplied by the host, never the payload.
    pub reviewer: Actor,
}

/// Outcome of an approval attempt. `Applying` is an explicit in-progress
/// result: the memory write may have committed while the receipt finalize
/// failed; retrying the same approval is safe (deterministic idempotency
/// key) and never re-promotes a second memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalApproveResult {
    pub proposal: OperatorProposal,
    /// Set when the promotion completed (status `approved`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<MemoryId>,
    /// True when the write deduplicated on retry (same idempotency key).
    pub deduplicated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalRejectRequest {
    pub id: OperatorProposalId,
    pub scope: Scope,
    pub expected_version: u64,
    pub reviewer: Actor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalRejectResult {
    pub proposal: OperatorProposal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_transitions_are_explicit() {
        use OperatorProposalStatus::*;
        assert!(Pending.can_transition_to(Applying));
        assert!(Applying.can_transition_to(Approved));
        assert!(Pending.can_transition_to(Rejected));
        // Terminal / illegal transitions.
        assert!(!Approved.can_transition_to(Rejected));
        assert!(!Rejected.can_transition_to(Applying));
        assert!(!Applying.can_transition_to(Rejected));
        assert!(!Pending.can_transition_to(Approved)); // only via applying
    }

    #[test]
    fn promotion_key_is_deterministic_per_proposal() {
        let mut p = OperatorProposal {
            id: OperatorProposalId::from("prop-1"),
            scope: Scope {
                tenant: "t".into(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            payload: ProposalMemoryWrite {
                kind: MemoryKind::Fact,
                content: MemoryContent {
                    text: "x".into(),
                    summary: None,
                    entities: vec![],
                    language: None,
                    format: None,
                    structured: None,
                    hash: None,
                },
                policy: Policy {
                    visibility: crate::Visibility::Workspace,
                    retention: crate::Retention::Durable,
                    sensitivity: None,
                    allowed_uses: vec![],
                    expires_at: None,
                    delete_mode: None,
                },
                links: vec![],
            },
            status: OperatorProposalStatus::Pending,
            submitted_by: Actor {
                id: crate::ActorId::from("a"),
                kind: crate::ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            payload_fingerprint: "fp".into(),
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: None,
            decision: None,
            promotion: None,
        };
        let first = p.promotion_idempotency_key();
        p.version += 1; // version changes must NOT change the key
        assert_eq!(first, p.promotion_idempotency_key());
        assert_eq!(first, "proposal-promotion-prop-1");
    }
}
