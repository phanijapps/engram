//! Scope matching for in-memory records and events.
//!
//! The in-memory adapter uses the same conservative visibility rule for records
//! and lifecycle events: tenant must match, and optional request scope fields
//! narrow visibility when present.

use engram_domain::Scope;

/// Returns true when a stored record or event is visible to a request scope.
///
/// Tenant is mandatory and must match exactly. Optional request scope fields
/// narrow visibility without requiring callers to supply every dimension.
pub(crate) fn scope_allows(record_scope: &Scope, request_scope: &Scope) -> bool {
    record_scope.tenant == request_scope.tenant
        && optional_scope_matches(&record_scope.subject, &request_scope.subject)
        && optional_scope_matches(&record_scope.workspace, &request_scope.workspace)
        && optional_scope_matches(&record_scope.session, &request_scope.session)
        && optional_scope_matches(&record_scope.environment, &request_scope.environment)
}

fn optional_scope_matches(record_value: &Option<String>, request_value: &Option<String>) -> bool {
    request_value
        .as_ref()
        .is_none_or(|value| record_value.as_ref() == Some(value))
}
