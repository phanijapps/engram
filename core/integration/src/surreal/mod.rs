//! SurrealDB backend for the integration facade.
//!
//! Everything under this module is engine-specific (it names `Surreal*`, holds
//! the Surreal adapter crates, and constructs the embedded SurrealKV store) and
//! is gated behind the `surreal` cargo feature. It is intentionally exempt from
//! the ADR-0022 engine-neutrality gate — the gate scans the neutral facade
//! files (`provider.rs`, `capability.rs`, …), not this engine submodule. See
//! the ADR-0022 amendment (2026-07-16): recipes are feature-gated engine
//! *submodules*, not separate crates, because a backend returns an
//! `EngramProvider` (owned by this crate) and a separate crate would form a
//! Cargo cycle with `EngramProvider::open`.
//!
//! [`bootstrap_surreal`] is the sole entry point, reached by
//! [`EngramProvider::open`](crate::EngramProvider::open) when the `surreal`
//! feature is enabled. Hosts select this backend declaratively via configuration
//! (compile with `--features surreal`) and reach every supported service through
//! the engine-neutral `Arc<dyn ...>` handles on the returned provider.

mod bootstrap;
mod memory;

pub(crate) use bootstrap::bootstrap_surreal;
pub use memory::SurrealMemoryService;
