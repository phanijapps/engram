//! Loaders for accepted portable contract examples.
//!
//! These helpers keep adapter tests pointed at the same checked-in v1 examples
//! without making any concrete store the owner of fixture parsing.

use engram_domain::*;

/// Accepted write-memory request example from the portable v1 contracts.
pub const WRITE_MEMORY_REQUEST_JSON: &str =
    include_str!("../../../contracts/v1/examples/write-memory-request.json");

/// Accepted retrieval request example from the portable v1 contracts.
pub const RETRIEVAL_REQUEST_JSON: &str =
    include_str!("../../../contracts/v1/examples/retrieval-request.json");

/// Invalid write-memory request with a missing scope tenant.
pub const INVALID_WRITE_MISSING_SCOPE_TENANT_JSON: &str = include_str!(
    "../../../contracts/v1/examples/invalid/write-memory-request.missing-scope-tenant.json"
);

/// Invalid write-memory request that asks for training export.
pub const INVALID_WRITE_TRAINING_EXPORT_JSON: &str = include_str!(
    "../../../contracts/v1/examples/invalid/write-memory-request.training-export.json"
);

/// Invalid retrieval request with a missing requester.
pub const INVALID_RETRIEVAL_MISSING_REQUESTER_JSON: &str =
    include_str!("../../../contracts/v1/examples/invalid/retrieval-request.missing-requester.json");

/// Parses the accepted write-memory request example into a domain request.
///
/// Adapter conformance tests should use this loader instead of embedding their
/// own path to the JSON fixture, so the accepted write example stays singular.
pub fn write_memory_request() -> Result<WriteMemoryRequest, serde_json::Error> {
    serde_json::from_str(WRITE_MEMORY_REQUEST_JSON)
}

/// Parses the accepted retrieval request example into a domain request.
///
/// The request is meant to run after `write_memory_request` has seeded the same
/// service, which keeps write/retrieve smoke tests portable across adapters.
pub fn retrieval_request() -> Result<RetrievalRequest, serde_json::Error> {
    serde_json::from_str(RETRIEVAL_REQUEST_JSON)
}

/// Attempts to parse the invalid missing-tenant write example.
///
/// This should fail during deserialization, before any concrete service is
/// invoked, proving schema-level required fields are enforced at the boundary.
pub fn invalid_write_missing_scope_tenant() -> Result<WriteMemoryRequest, serde_json::Error> {
    serde_json::from_str(INVALID_WRITE_MISSING_SCOPE_TENANT_JSON)
}

/// Parses the structurally valid write example rejected by v1 behavior.
///
/// Unlike missing-field fixtures, this payload can deserialize and must be
/// rejected by service validation because the requested allowed use is invalid.
pub fn invalid_write_training_export() -> Result<WriteMemoryRequest, serde_json::Error> {
    serde_json::from_str(INVALID_WRITE_TRAINING_EXPORT_JSON)
}

/// Attempts to parse the invalid missing-requester retrieval example.
///
/// This should fail during deserialization, keeping malformed retrieval inputs
/// out of store adapters and native transports.
pub fn invalid_retrieval_missing_requester() -> Result<RetrievalRequest, serde_json::Error> {
    serde_json::from_str(INVALID_RETRIEVAL_MISSING_REQUESTER_JSON)
}
