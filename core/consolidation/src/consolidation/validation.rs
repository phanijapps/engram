//! Request validation for dry-run consolidation.
//!
//! This module owns preflight checks that must happen before any task planning.
//! It keeps invalid scope, requester, time-window, and mutating-mode requests
//! from producing misleading run reports.

use engram_domain::ConsolidationRequest;

use crate::{CoreError, CoreResult};

/// Rejects invalid dry-run consolidation requests before planning begins.
///
/// Validation is intentionally limited to request shape and first-slice safety.
/// Authorization, retention policy, and task-specific mutation checks belong to
/// future services that actually read or write repositories.
pub(crate) fn validate_request(request: &ConsolidationRequest) -> CoreResult<()> {
    validate_request_shape(request)?;
    if matches!(request.dry_run, Some(false)) {
        return invalid("dry-run consolidation service cannot execute mutating requests");
    }

    Ok(())
}

/// Rejects invalid mutating consolidation requests before any gate executes.
///
/// Mutation is opt-in only: callers must send `dryRun=false` explicitly. A
/// missing flag is rejected so dry-run and mutating services cannot be confused
/// by defaults or omitted fields.
pub(crate) fn validate_mutating_request(request: &ConsolidationRequest) -> CoreResult<()> {
    validate_request_shape(request)?;
    if !matches!(request.dry_run, Some(false)) {
        return invalid("mutating consolidation requires explicit dryRun=false");
    }

    Ok(())
}

pub(crate) fn validate_planning_request(request: &ConsolidationRequest) -> CoreResult<()> {
    validate_request_shape(request)
}

fn validate_request_shape(request: &ConsolidationRequest) -> CoreResult<()> {
    if request.scope.tenant.trim().is_empty() {
        return invalid("scope.tenant is required");
    }

    if request.requester.actor.id.as_str().trim().is_empty() {
        return invalid("requester.actor.id is required");
    }

    if let (Some(since), Some(until)) = (request.since, request.until) {
        if since > until {
            return invalid("since must be before or equal to until");
        }
    }

    Ok(())
}

fn invalid<T>(reason: impl Into<String>) -> CoreResult<T> {
    Err(CoreError::InvalidRequest {
        reason: reason.into(),
    })
}
