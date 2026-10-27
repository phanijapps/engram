//! Proposal service — submit / approve / reject orchestration.
//!
//! Approval is an orchestration operation, not a repository state flip:
//! claim (CAS) → policy-checked `MemoryService::write_memory` with a
//! deterministic per-proposal idempotency key → finalize (CAS). There is no
//! cross-store transaction and no rollback: if the memory commits but the
//! finalize fails, the proposal stays visibly `applying` and the approval
//! result reports the memory id so a retry (or recovery process) can safely
//! replay the same idempotent write and finalize.

use std::sync::Arc;

use chrono::Utc;
use engram_domain::{
    Actor, OperatorProposal, OperatorProposalId, OperatorProposalStatus, ProposalApproveRequest,
    ProposalApproveResult, ProposalDecision, ProposalListRequest, ProposalListResult,
    ProposalPromotion, ProposalRejectRequest, ProposalRejectResult, ProposalSubmitRequest,
    ProposalSubmitResult, Provenance, Requester, WriteMemoryRequest,
};
use engram_memory::MemoryService;
use engram_runtime::{CoreError, CoreResult};

use crate::authorization::denial_id;
use crate::payload_fingerprint;
use crate::{ProposalAuthorizer, ProposalRepository};

/// Proposal queue orchestration over an injected memory service, repository,
/// and operator authorizer.
pub struct ProposalService {
    memory: Arc<dyn MemoryService>,
    repository: Arc<dyn ProposalRepository>,
    authorizer: Arc<dyn ProposalAuthorizer>,
}

impl ProposalService {
    pub fn new(
        memory: Arc<dyn MemoryService>,
        repository: Arc<dyn ProposalRepository>,
        authorizer: Arc<dyn ProposalAuthorizer>,
    ) -> Self {
        Self {
            memory,
            repository,
            authorizer,
        }
    }

    /// Submits an inert proposal. The payload is fingerprinted and frozen;
    /// edits require resubmission.
    pub async fn submit(&self, request: ProposalSubmitRequest) -> CoreResult<ProposalSubmitResult> {
        if !self
            .authorizer
            .can_submit(&request.submitted_by, &request.scope)
            .await?
        {
            return Err(CoreError::PolicyDenied {
                reason: "actor may not submit proposals in this scope".to_owned(),
            });
        }
        let now = Utc::now();
        let proposal = OperatorProposal {
            // Unique, non-guessable-enough local id: timestamp + payload
            // fingerprint prefix (submission keys provide the idempotent
            // replay path; ids need uniqueness, not secrecy).
            id: OperatorProposalId::from(format!(
                "proposal-{}-{}",
                now.timestamp_nanos_opt().unwrap_or_default(),
                &payload_fingerprint(&request.payload)[..12]
            )),
            scope: request.scope,
            payload: request.payload,
            status: OperatorProposalStatus::Pending,
            submitted_by: request.submitted_by,
            payload_fingerprint: String::new(), // set below
            version: 1,
            created_at: now,
            updated_at: None,
            decision: None,
            promotion: None,
        };
        let mut proposal = proposal;
        proposal.payload_fingerprint = payload_fingerprint(&proposal.payload);
        let stored = self.repository.insert_proposal(proposal).await?;
        Ok(ProposalSubmitResult { proposal: stored })
    }

    /// Lists queue records (review access required; listing is not a
    /// retrieval lane and never feeds recall).
    pub async fn list(&self, request: ProposalListRequest) -> CoreResult<ProposalListResult> {
        // The listing actor is the MCP/SDK host's current actor; the request
        // carries only the scope. Hosts wanting per-actor review gating wrap
        // this call with their own authorization.
        let actor = Actor {
            id: engram_domain::ActorId::from("host"),
            kind: engram_domain::ActorKind::System,
            display_name: Some("host".to_owned()),
            metadata: None,
        };
        if !self.authorizer.can_review(&actor, &request.scope).await? {
            return Err(CoreError::PolicyDenied {
                reason: "actor may not review proposals in this scope".to_owned(),
            });
        }
        let limit = request.limit.clamp(1, 100);
        let items = self
            .repository
            .list_proposals(
                &request.scope,
                request.status,
                request.after.as_ref(),
                limit,
            )
            .await?;
        let next_cursor = if items.len() as u32 == limit {
            items.last().map(|p| p.id.clone())
        } else {
            None
        };
        Ok(ProposalListResult { items, next_cursor })
    }

    /// Approves a proposal: claim → policy-checked memory write → finalize.
    /// See the module docs for the failure contract.
    pub async fn approve(
        &self,
        request: ProposalApproveRequest,
    ) -> CoreResult<ProposalApproveResult> {
        if !self
            .authorizer
            .can_approve(&request.reviewer, &request.scope)
            .await?
        {
            return Err(CoreError::PolicyDenied {
                reason: format!(
                    "actor may not approve proposal {} in this scope",
                    denial_id(&request.id)
                ),
            });
        }
        let current = self
            .repository
            .get_proposal(&request.id, &request.scope)
            .await?
            .ok_or_else(|| CoreError::NotFound {
                target_type: "operator_proposal",
                target_id: request.id.to_string(),
            })?;

        match current.status {
            OperatorProposalStatus::Approved => {
                // Idempotent replay: return the existing receipt.
                let memory_id = current.promotion.as_ref().map(|p| p.memory_id.clone());
                return Ok(ProposalApproveResult {
                    proposal: current,
                    memory_id,
                    deduplicated: true,
                });
            }
            OperatorProposalStatus::Rejected => {
                return Err(CoreError::Conflict {
                    reason: "proposal is rejected; approval is not possible".to_owned(),
                });
            }
            OperatorProposalStatus::Applying => {
                // Recovery path: replay the same idempotent write, then
                // finalize. The write deduplicates on the promotion key.
            }
            OperatorProposalStatus::Pending => {
                if current.version != request.expected_version {
                    return Err(CoreError::Conflict {
                        reason: format!(
                            "version mismatch: expected {}, got {}",
                            current.version, request.expected_version
                        ),
                    });
                }
                self.repository
                    .claim_approval(&request.id, &request.scope, request.expected_version)
                    .await?;
            }
        }

        // Promote through the normal policy-checked write path. The
        // deterministic per-proposal idempotency key makes retries replay
        // the same memory (never a shared batch key, never a second record).
        let payload = current.payload.clone();
        let promotion_key = current.promotion_idempotency_key();
        let write = WriteMemoryRequest {
            kind: payload.kind,
            content: payload.content,
            scope: current.scope.clone(),
            requester: Requester {
                actor: request.reviewer.clone(),
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            provenance: Provenance {
                source: "operator-proposal".to_owned(),
                actor: request.reviewer.clone(),
                observed_at: Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: Some("proposal_approval".to_owned()),
            },
            policy: payload.policy,
            links: payload.links,
            verdict: None,
            idempotency_key: Some(promotion_key.clone()),
        };
        let written = match self.memory.write_memory(write).await {
            Ok(response) => response,
            Err(e) => {
                // Claim held, nothing committed: surface the error. The
                // proposal stays `applying`; a retry re-runs this exact path.
                return Err(e);
            }
        };
        let promotion = ProposalPromotion {
            memory_id: written.record.id.clone(),
            event_id: written.event.id.clone(),
            idempotency_key: promotion_key,
            applied_at: Utc::now(),
        };
        let deduplicated = written.deduplicated.unwrap_or(false);
        // Refetch for the CURRENT version (claim bumped it) and finalize.
        let applying = self
            .repository
            .get_proposal(&request.id, &request.scope)
            .await?
            .ok_or_else(|| CoreError::NotFound {
                target_type: "operator_proposal",
                target_id: request.id.to_string(),
            })?;
        match self
            .repository
            .record_promotion(&request.id, &request.scope, applying.version, promotion)
            .await
        {
            Ok(proposal) => {
                let memory_id = proposal.promotion.as_ref().map(|p| p.memory_id.clone());
                Ok(ProposalApproveResult {
                    proposal,
                    memory_id,
                    deduplicated,
                })
            }
            Err(_) => {
                // Memory committed, finalize failed: report in-progress with
                // the memory id and the still-`applying` proposal — never a
                // clean error that implies nothing was written. Retrying
                // approve replays the idempotent write and finalizes.
                Ok(ProposalApproveResult {
                    proposal: applying,
                    memory_id: Some(written.record.id.clone()),
                    deduplicated,
                })
            }
        }
    }

    /// Rejects a pending proposal. Rejection after an approval claim is a
    /// conflict (the memory may have committed); rejection cannot erase an
    /// already promoted memory — normal forget semantics apply to it.
    pub async fn reject(&self, request: ProposalRejectRequest) -> CoreResult<ProposalRejectResult> {
        if !self
            .authorizer
            .can_approve(&request.reviewer, &request.scope)
            .await?
        {
            return Err(CoreError::PolicyDenied {
                reason: format!(
                    "actor may not reject proposal {} in this scope",
                    denial_id(&request.id)
                ),
            });
        }
        let current = self
            .repository
            .get_proposal(&request.id, &request.scope)
            .await?
            .ok_or_else(|| CoreError::NotFound {
                target_type: "operator_proposal",
                target_id: request.id.to_string(),
            })?;
        match current.status {
            OperatorProposalStatus::Rejected => {
                return Ok(ProposalRejectResult { proposal: current });
            }
            OperatorProposalStatus::Applying | OperatorProposalStatus::Approved => {
                return Err(CoreError::Conflict {
                    reason: match current.status {
                        OperatorProposalStatus::Applying => format!(
                            "approval in progress for {}; rejection cannot overtake a possibly-committed promotion",
                            denial_id(&request.id)
                        ),
                        _ => format!(
                            "proposal {} already promoted a memory; use memory forget semantics instead",
                            denial_id(&request.id)
                        ),
                    },
                });
            }
            OperatorProposalStatus::Pending => {}
        }
        if current.version != request.expected_version {
            return Err(CoreError::Conflict {
                reason: format!(
                    "version mismatch: expected {}, got {}",
                    current.version, request.expected_version
                ),
            });
        }
        let decision = ProposalDecision {
            reviewer: request.reviewer,
            decided_at: Utc::now(),
            reason: request.reason,
        };
        let proposal = self
            .repository
            .reject_proposal(
                &request.id,
                &request.scope,
                request.expected_version,
                decision,
            )
            .await?;
        Ok(ProposalRejectResult { proposal })
    }
}
