//! Request validation for in-memory service operations.
//!
//! These checks enforce behavior-level v1 constraints that are not fully
//! captured by deserialization alone, such as forbidding `training_export` on
//! v1 writes and rejecting zero-valued retrieval budgets.

use engram_core::{CoreError, CoreResult};
use engram_domain::{AllowedUse, ForgetRequest, RetrievalRequest, WriteMemoryRequest};

/// Validates behavior-level constraints for v1 memory writes.
///
/// Deserialization proves the payload shape. This function enforces additional
/// service rules such as non-empty text/source and the v1 `training_export`
/// prohibition.
pub(crate) fn validate_write_request(request: &WriteMemoryRequest) -> CoreResult<()> {
    if request.scope.tenant.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "scope.tenant is required".to_owned(),
        });
    }
    if request.content.text.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "content.text is required".to_owned(),
        });
    }
    if request.provenance.source.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "provenance.source is required".to_owned(),
        });
    }
    if request
        .policy
        .allowed_uses
        .contains(&AllowedUse::TrainingExport)
    {
        return Err(CoreError::InvalidRequest {
            reason: "policy.allowedUses must not include training_export in v1".to_owned(),
        });
    }
    Ok(())
}

/// Validates behavior-level constraints for retrieval requests.
///
/// The in-memory baseline rejects empty queries and zero-valued limits before
/// any candidate scan so adapters share the same failure behavior.
pub(crate) fn validate_retrieval_request(request: &RetrievalRequest) -> CoreResult<()> {
    if request.scope.tenant.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "scope.tenant is required".to_owned(),
        });
    }
    if request.query.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "query is required".to_owned(),
        });
    }
    if request.limit == Some(0) {
        return Err(CoreError::InvalidRequest {
            reason: "limit must be positive when supplied".to_owned(),
        });
    }
    if let Some(budget) = &request.budget
        && (budget.max_items == Some(0)
            || budget.max_tokens == Some(0)
            || budget.max_bytes == Some(0))
    {
        return Err(CoreError::InvalidRequest {
            reason: "budget limits must be positive when supplied".to_owned(),
        });
    }
    Ok(())
}

/// Validates behavior-level constraints for forget requests.
///
/// Shape validation proves the request can be deserialized. This function keeps
/// lifecycle behavior deterministic by rejecting empty scope and target values
/// before any state mutation is attempted.
pub(crate) fn validate_forget_request(request: &ForgetRequest) -> CoreResult<()> {
    if request.scope.tenant.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "scope.tenant is required".to_owned(),
        });
    }
    if request.target_id.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "targetId is required".to_owned(),
        });
    }
    Ok(())
}
