//! Graph-analytics primitives.
//!
//! Pure algorithms over a generic directed edge list, decoupled from domain
//! types. Callers map `KnowledgeRelationship` (or any edge source) to
//! `(source, target)` id pairs at the call site, so the algorithms are reusable
//! and testable in isolation. No dependencies.
//!
//! Currently: PageRank centrality. Betweenness (B4) and Louvain community
//! detection (B5) land as follow-on micro-specs in this crate.

mod betweenness;
mod communities;
mod pagerank;
mod reachability;

pub use betweenness::betweenness;
pub use communities::communities;
pub use pagerank::pagerank;
pub use reachability::{ancestors, in_degree, shortest_path};
