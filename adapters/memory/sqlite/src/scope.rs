//! Scope matching and SQL scope binding helpers.
//!
//! SQL rows keep scope columns for filtering, while JSON payloads preserve the
//! full accepted contract record. The visibility rule mirrors the in-memory
//! adapter: tenant must match, and optional request fields narrow visibility.

use engram_domain::Scope;

/// Returns true when a stored SQL row is visible to a request scope.
///
/// Tenant must match exactly. Optional request scope fields narrow visibility
/// without requiring every scope dimension to be supplied.
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
