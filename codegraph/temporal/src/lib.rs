//! Codegraph temporal scoring engine.
//!
//! On-top codegraph crate (RFC-0012): ranks versioned symbols by recency,
//! blast-radius-weighted impact, and a compound blend — the first three of
//! memtrace's six temporal scoring modes. Pure math over a caller-built
//! [`VersionedSymbol`] input (validity interval from ADR-0019, in/out-degree
//! from the call graph). The `novel` / `directional` / `overview` modes (which
//! need a change-diff / baseline model) are deferred. Depends only on `chrono`.

mod scoring;

pub use scoring::{VersionedSymbol, compound, impact, recent};
