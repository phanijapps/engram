//! Shared runtime contracts used by Engram behavior crates.
//!
//! This crate intentionally stays below memory, knowledge, and core
//! orchestration. It owns portable errors and dependency traits that would
//! otherwise create circular dependencies between independently stored memory
//! and knowledge systems.

pub mod error;
pub mod redaction;

use engram_domain::{Id, Policy, Requester, Scope, Timestamp};

pub use error::{CoreError, CoreResult, DiagnosticError};

/// Supplies timestamps to services without binding them to system time.
///
/// Production adapters usually delegate to a UTC system clock. Tests, replay
/// harnesses, and deterministic consolidation runs should provide scripted
/// clocks so ordering, retention, and audit behavior can be reproduced exactly.
pub trait Clock: Send + Sync {
    /// Returns the current UTC timestamp for newly created domain records.
    fn now(&self) -> Timestamp;
}

/// Generates opaque identifiers for domain entities at behavior boundaries.
///
/// Implementations may use UUIDs, ULIDs, content hashes, or another strategy,
/// but callers must treat the returned value as opaque. Scope, authorization,
/// timestamp, and storage-location semantics belong in typed domain fields, not
/// in the identifier text.
pub trait IdGenerator: Send + Sync {
    /// Creates a new opaque identifier for the named entity type.
    fn new_id(&self, entity_type: &'static str) -> Id;
}

/// Decides whether a record scope is eligible for a request scope.
///
/// This is a structural visibility check, not a full authorization decision.
/// Adapters use it before policy checks to avoid mixing tenants, subjects, or
/// workspaces across retrieval and maintenance paths.
pub trait ScopeMatcher: Send + Sync {
    /// Returns true when `record_scope` may be considered for `request_scope`.
    fn is_visible_scope(&self, request_scope: &Scope, record_scope: &Scope) -> bool;
}

/// Enforces policy before durable mutations or retrieval composition.
///
/// Implementations must keep denials explicit and stable enough for audit and
/// evaluation. Physical storage isolation may add stricter checks, but it must
/// not bypass this logical policy boundary.
pub trait PolicyAuthorizer: Send + Sync {
    /// Checks whether `requester` may create or update a record in `scope`.
    fn can_write(&self, requester: &Requester, scope: &Scope, policy: &Policy) -> CoreResult<()>;

    /// Checks whether `requester` may retrieve a record governed by `policy`.
    fn can_retrieve(&self, requester: &Requester, scope: &Scope, policy: &Policy)
    -> CoreResult<()>;

    /// Checks whether `requester` may apply the requested deletion behavior.
    fn can_forget(&self, requester: &Requester, scope: &Scope, policy: &Policy) -> CoreResult<()>;
}
