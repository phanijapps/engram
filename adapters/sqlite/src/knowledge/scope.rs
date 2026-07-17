//! Scope matching for knowledge rows.
//!
//! Tenant must match exactly; optional request scope fields narrow visibility.
//! Mirrors the rule used by the memory and in-memory knowledge adapters.

use engram_domain::Scope;

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
