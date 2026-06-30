//! Request validation for SQL service operations.
//!
//! These checks mirror the in-memory baseline so adapter conformance does not
//! depend on which persistence backend receives the request.

use engram_core::{CoreError, CoreResult};
use engram_domain::{AllowedUse, ForgetRequest, RetrievalRequest, WriteMemoryRequest};

/// Validates behavior-level constraints for v1 memory writes.
///
/// These checks match the in-memory baseline and catch service rules that
/// deserialization alone cannot express, including the v1 training-export ban.
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
/// The SQL baseline rejects empty queries and zero budgets before touching
/// storage so adapters share the same caller-facing failure behavior.
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
/// Forget validation happens before authorization or mutation to avoid partial
/// lifecycle side effects from malformed target or scope data.
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
