//! Graph-analytics primitives.
//!
//! Pure algorithms over a generic directed edge list, decoupled from domain
//! types. Callers map `KnowledgeRelationship` (or any edge source) to
//! `(source, target)` id pairs at the call site, so the algorithms are reusable
//! and testable in isolation. No dependencies.
//!
//! Currently: PageRank centrality. Betweenness (B4) and Louvain community
//! detection (B5) land as follow-on micro-specs in this crate.

mod pagerank;

pub use pagerank::pagerank;
