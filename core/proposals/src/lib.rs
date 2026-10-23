//! engram-proposals — operator proposal queue behavior (wicked-estate
//! adoption, slice 2).
//!
//! Storage-neutral orchestration for inert, reviewable pending writes:
//! a repository port ([`ProposalRepository`]), an operator authorization
//! port ([`ProposalAuthorizer`]), and the [`ProposalService`] that owns
//! submit / approve / reject. Approval routes through the injected
//! `MemoryService::write_memory` — never around it — so the existing policy
//! authorizer and lifecycle events apply to every promoted memory. Proposals
//! are structurally excluded from retrieval: this crate exposes no read path
//! that feeds recall, and no proposal repository handle is handed to any
//! retrieval lane.

mod authorization;
mod repository;
mod service;

pub use authorization::{
    AllowAllProposalAuthorizer, DenyAllProposalAuthorizer, ProposalAuthorizer,
};
pub use repository::ProposalRepository;
pub use service::ProposalService;

/// Deterministic fingerprint of the immutable proposal payload (version
/// pin): SHA-256 over the canonical JSON of the payload.
pub fn payload_fingerprint(payload: &engram_domain::ProposalMemoryWrite) -> String {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::to_vec(payload).expect("payload serializes");
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
