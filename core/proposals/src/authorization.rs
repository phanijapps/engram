//! Operator authorization port.
//!
//! The existing policy gates are NOT operator authentication: several
//! default memory-service bootstrap paths construct a permissive policy
//! authorizer, so calling `MemoryService` is necessary but not sufficient. Operator authority — who may submit, who may approve — is a
//! host concern injected here; an identity-looking JSON payload from the
//! proposing agent is never proof of authority (an `ActorKind::User` in a
//! payload is data, not authentication).

use async_trait::async_trait;
use engram_domain::{Actor, OperatorProposalId, Scope};
use engram_runtime::CoreResult;

/// Authorization decisions for the proposal queue, made by the trusted host.
#[async_trait]
pub trait ProposalAuthorizer: Send + Sync {
    /// May this actor submit proposals in this scope? (Read-only content
    /// workers may hold submit permission without holding write or approve.)
    async fn can_submit(&self, actor: &Actor, scope: &Scope) -> CoreResult<bool>;

    /// May this actor approve (or reject) proposals in this scope? Supplied
    /// by the trusted host context, never derived from the proposal payload.
    async fn can_approve(&self, actor: &Actor, scope: &Scope) -> CoreResult<bool>;

    /// May this actor read queue contents (list/get) in this scope?
    async fn can_review(&self, actor: &Actor, scope: &Scope) -> CoreResult<bool>;
}

/// Test/local authorizer: everything allowed. NEVER wire behind an untrusted
/// boundary — the MCP server constructs it only for explicitly operator-
/// enabled local configurations.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAllProposalAuthorizer;

#[async_trait]
impl ProposalAuthorizer for AllowAllProposalAuthorizer {
    async fn can_submit(&self, _actor: &Actor, _scope: &Scope) -> CoreResult<bool> {
        Ok(true)
    }
    async fn can_approve(&self, _actor: &Actor, _scope: &Scope) -> CoreResult<bool> {
        Ok(true)
    }
    async fn can_review(&self, _actor: &Actor, _scope: &Scope) -> CoreResult<bool> {
        Ok(true)
    }
}

/// Deny-everything authorizer: the safe default for tests proving that
/// denial leaves no promoted memory.
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyAllProposalAuthorizer;

#[async_trait]
impl ProposalAuthorizer for DenyAllProposalAuthorizer {
    async fn can_submit(&self, _actor: &Actor, _scope: &Scope) -> CoreResult<bool> {
        Ok(false)
    }
    async fn can_approve(&self, _actor: &Actor, _scope: &Scope) -> CoreResult<bool> {
        Ok(false)
    }
    async fn can_review(&self, _actor: &Actor, _scope: &Scope) -> CoreResult<bool> {
        Ok(false)
    }
}

/// Helper for error messages.
pub(crate) fn denial_id(id: &OperatorProposalId) -> String {
    id.to_string()
}
