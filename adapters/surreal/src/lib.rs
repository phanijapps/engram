//! engram-store-surreal — the consolidated SurrealDB backend for engram.
//!
//! **One crate per backend** (ADR-0022 amended 2026-07-16): ALL Surreal database
//! operations for every capability live in this single crate — memory,
//! knowledge, belief, hierarchy, vectors, consolidation — sharing one embedded
//! SurrealKV connection. This replaces the per-capability adapter-crate grid
//! (`adapters/<capability>/<engine>`) for consolidated engines: a single-engine
//! backend is one crate at `adapters/<engine>/`. Future backends follow the
//! same convention: `engram-store-sqlite`, `engram-store-surreal`,
//! `engram-store-mixed` (e.g. lancedb + neo4j composed), etc.
//!
//! The thin recipe wiring that returns an `EngramProvider` (`bootstrap_surreal`)
//! lives in `engram-integration`, not here: it returns a facade-owned type and a
//! crate that did so would form a Cargo cycle with `EngramProvider::open`.
//!
//! ADR-0022: this crate is engine-specific (names `Surreal*`, holds surrealdb)
//! and is exempt from the engine-neutrality gate — the gate scans the neutral
//! facade/port crates, not engine adapter crates.

mod util;

pub mod belief;
pub mod connection;
pub mod hierarchy;
pub mod memory;

pub use belief::SurrealBeliefStore;
pub use connection::SurrealConnection;
pub use hierarchy::SurrealHierarchyStore;
pub use memory::SurrealMemoryService;
