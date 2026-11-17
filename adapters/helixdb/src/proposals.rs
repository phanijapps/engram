//! Durable proposal CAS only; approval orchestration stays in ProposalService.
use crate::{HelixStore, records::*};
use async_trait::async_trait;
use engram_domain::*;
use engram_proposals::ProposalRepository;
use engram_runtime::{CoreError, CoreResult};
fn conflict() -> CoreError {
    CoreError::Conflict {
        reason: "proposal version/status or immutable payload conflict".into(),
    }
}
impl HelixStore {
    fn transition_proposal(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
        version: u64,
        next: OperatorProposalStatus,
        update: impl FnOnce(&mut OperatorProposal) -> CoreResult<()>,
    ) -> CoreResult<OperatorProposal> {
        self.check_scope(scope).map_err(core)?;
        self.domain_atomic(|s| {
            let mut v = s
                .get::<OperatorProposal>(PROPOSAL, id.as_str())
                .map_err(core)?
                .ok_or_else(|| missing("operator_proposal", id))?;
            if v.status == next && next != OperatorProposalStatus::Applying {
                update(&mut v)?;
                return Ok(v);
            }
            if v.version != version || !v.status.can_transition_to(next) {
                return Err(conflict());
            }
            update(&mut v)?;
            v.status = next;
            v.version = v.version.checked_add(1).ok_or_else(conflict)?;
            v.updated_at = Some(chrono::Utc::now());
            s.put(PROPOSAL, &v).map_err(core)
        })
    }
}
#[async_trait]
impl ProposalRepository for HelixStore {
    async fn insert_proposal(&self, v: OperatorProposal) -> CoreResult<OperatorProposal> {
        self.check_scope(&v.scope).map_err(core)?;
        self.domain_atomic(|s| {
            if let Some(old) = s
                .get::<OperatorProposal>(PROPOSAL, v.id.as_str())
                .map_err(core)?
            {
                if old.payload_fingerprint != v.payload_fingerprint || old.payload != v.payload {
                    return Err(conflict());
                }
                return Ok(old);
            }
            if v.status != OperatorProposalStatus::Pending
                || v.promotion.is_some()
                || v.decision.is_some()
                || engram_proposals::payload_fingerprint(&v.payload) != v.payload_fingerprint
            {
                return Err(conflict());
            }
            s.put(PROPOSAL, &v).map_err(core)
        })
    }
    async fn get_proposal(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
    ) -> CoreResult<Option<OperatorProposal>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(PROPOSAL, id.as_str()).map_err(core)
    }
    async fn list_proposals(
        &self,
        scope: &Scope,
        status: Option<OperatorProposalStatus>,
        after: Option<&OperatorProposalId>,
        limit: u32,
    ) -> CoreResult<Vec<OperatorProposal>> {
        if limit == 0 || limit > 500 {
            return Err(CoreError::InvalidRequest {
                reason: "invalid bounded proposal page".into(),
            });
        }
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        let mut rows = self.all::<OperatorProposal>(PROPOSAL).map_err(core)?;
        rows.retain(|v| status.is_none_or(|x| x == v.status));
        rows.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        if let Some(id) = after {
            let at =
                rows.iter()
                    .position(|v| &v.id == id)
                    .ok_or_else(|| CoreError::InvalidRequest {
                        reason: "proposal cursor not in selected partition/status".into(),
                    })?;
            rows.drain(..=at);
        }
        rows.truncate(limit as usize);
        Ok(rows)
    }
    async fn claim_approval(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
        version: u64,
    ) -> CoreResult<OperatorProposal> {
        self.transition_proposal(id, scope, version, OperatorProposalStatus::Applying, |_| {
            Ok(())
        })
    }
    async fn record_promotion(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
        version: u64,
        promotion: ProposalPromotion,
    ) -> CoreResult<OperatorProposal> {
        self.transition_proposal(id, scope, version, OperatorProposalStatus::Approved, |v| {
            if v.status == OperatorProposalStatus::Approved {
                if v.promotion.as_ref() != Some(&promotion) {
                    return Err(conflict());
                }
            } else {
                if promotion.idempotency_key != v.promotion_idempotency_key() {
                    return Err(conflict());
                }
                v.promotion = Some(promotion)
            }
            Ok(())
        })
    }
    async fn reject_proposal(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
        version: u64,
        decision: ProposalDecision,
    ) -> CoreResult<OperatorProposal> {
        self.transition_proposal(id, scope, version, OperatorProposalStatus::Rejected, |v| {
            if v.status != OperatorProposalStatus::Rejected {
                v.decision = Some(decision)
            }
            Ok(())
        })
    }
}
