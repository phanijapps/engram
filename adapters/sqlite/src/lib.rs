//! engram-store-sqlite — the consolidated SQLite backend for engram.
//!
//! **One crate per backend** (ADR-0022 amended 2026-07-16): ALL SQLite database
//! operations live in this single crate. T0 is a **re-export facade** — it
//! re-exports the existing per-capability adapter crates (memory, knowledge,
//! belief, hierarchy, vector) under one import surface, with ZERO code moved.
//! Their code then folds into this crate incrementally (plan T1-T5), and the
//! `Sql*` glue joins them (T6). Future backends follow the same convention:
//! `engram-store-surreal`, `engram-store-mixed`, …
//!
//! The thin `bootstrap_sqlite` wiring (which returns the facade-owned
//! `EngramProvider`) stays in `engram-integration`, not here — it would form a
//! Cargo cycle with `EngramProvider::open`.
//!
//! Engine-agnostic adapters (Tantivy lexical, associative-graph,
//! community-summary, decay, ingest) are NOT part of this crate — they are
//! shared with the Surreal backend and any future backend.

pub use engram_store_hierarchy_sqlite::*;

// Belief cell — folded in from engram-store-belief-sqlite (T3).
pub mod belief;
pub use belief::*;
pub use engram_store_vector::*;

// Knowledge cell — folded in from engram-store-knowledge-sqlite (T2).
pub mod knowledge;
pub use knowledge::*;

// Memory cell — folded in from engram-store-sql (consolidation T1). Its code
// lives in `memory/`; re-exported at the crate root so `engram_store_sqlite::*`
// and internal `crate::memory::*` both resolve.
pub mod memory;
pub use memory::*;
