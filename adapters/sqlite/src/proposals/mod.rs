//! SQLite-backed operator proposal queue (wicked-estate adoption, slice 2).
//!
//! Storage-only CAS state machine: `pending → applying → approved` and
//! `pending → rejected` transitions are guarded by `(id, version, status)`
//! UPDATE predicates, so exactly one claimant can promote a proposal. The
//! promotion write itself happens in `engram-proposals`' service through the
//! memory store — never here.

mod schema;
mod scope;

pub use repository::SqlProposalStore;

mod repository;
