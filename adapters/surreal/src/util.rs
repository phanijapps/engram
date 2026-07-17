//! Shared helpers for Surreal cells: scope matching, error mapping, and the
//! `data`-field wrapper for lossless DTO persistence.

use engram_domain::Scope;
use engram_runtime::CoreError;
use serde::Deserialize;

/// Scope-visibility rule (mirrors the SQLite adapters' `scope::scope_allows`):
/// tenant must match exactly; optional request fields narrow.
pub(crate) fn scope_allows(record_scope: &Scope, request_scope: &Scope) -> bool {
    record_scope.tenant == request_scope.tenant
        && optional_matches(&record_scope.subject, &request_scope.subject)
        && optional_matches(&record_scope.workspace, &request_scope.workspace)
        && optional_matches(&record_scope.session, &request_scope.session)
        && optional_matches(&record_scope.environment, &request_scope.environment)
}

fn optional_matches(record: &Option<String>, request: &Option<String>) -> bool {
    request
        .as_ref()
        .is_none_or(|value| record.as_ref() == Some(value))
}

pub(crate) fn surreal_err(error: surrealdb::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "surreal".to_owned(),
        message: error.to_string(),
    }
}

/// Deserialize shim for the `SELECT data FROM ...` pattern — each Surreal record
/// stores the full DTO under `data` to avoid collisions with Surreal metadata.
#[derive(Deserialize)]
pub(crate) struct DataWrapper<T> {
    pub(crate) data: T,
}
