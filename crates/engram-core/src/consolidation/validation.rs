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
    if request.scope.tenant.trim().is_empty() {
        return invalid("scope.tenant is required");
    }

    if request.requester.actor.id.as_str().trim().is_empty() {
        return invalid("requester.actor.id is required");
    }

    if matches!(request.dry_run, Some(false)) {
        return invalid("dry-run consolidation service cannot execute mutating requests");
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
