//! Proposal repository port — storage-neutral CAS state machine.

use async_trait::async_trait;
use engram_domain::{
    OperatorProposal, OperatorProposalId, OperatorProposalStatus, ProposalDecision,
    ProposalPromotion, Scope,
};
use engram_runtime::CoreResult;

/// Persistence port for the operator proposal queue.
///
/// State transitions are compare-and-set on `(id, expected_version)`: exactly
/// one caller can move `pending → applying`, and only that caller can
/// finalize `applying → approved` with the promotion receipt. Rejection is
/// legal only from `pending` — an approval that may have committed a memory
/// can never be rejected afterwards.
// async_trait codegen stamps #[must_use] on the boxed-future helper while
// CoreResult is already must_use — newer clippy flags the combination
// (same allow as the procedures port).
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait ProposalRepository: Send + Sync {
    /// Inserts a proposal (idempotent on the scope-bound submission key:
    /// a resubmit returns the existing record).
    async fn insert_proposal(&self, proposal: OperatorProposal) -> CoreResult<OperatorProposal>;

    /// Looks up one proposal by id inside the supplied scope.
    async fn get_proposal(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
    ) -> CoreResult<Option<OperatorProposal>>;

    /// Lists proposals for a scope, optionally filtered by status, ordered
    /// by creation (then id), paged by an opaque id cursor.
    async fn list_proposals(
        &self,
        scope: &Scope,
        status: Option<OperatorProposalStatus>,
        after: Option<&OperatorProposalId>,
        limit: u32,
    ) -> CoreResult<Vec<OperatorProposal>>;

    /// CAS `pending → applying` (approval claim). Fails with
    /// [`CoreError::Conflict`] when the version or status does not match.
    async fn claim_approval(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
        expected_version: u64,
    ) -> CoreResult<OperatorProposal>;

    /// CAS `applying → approved` with the promotion receipt. Idempotent:
    /// finalizing an already-approved proposal with the same receipt
    /// returns it unchanged.
    async fn record_promotion(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
        expected_version: u64,
        promotion: ProposalPromotion,
    ) -> CoreResult<OperatorProposal>;

    /// CAS `pending → rejected` with the decision record. Idempotent on the
    /// terminal state.
    async fn reject_proposal(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
        expected_version: u64,
        decision: ProposalDecision,
    ) -> CoreResult<OperatorProposal>;
}
