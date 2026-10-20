//! Community-summary retrieval — a `RetrievalIndex` for `RetrievalMode::Graph`.
//!
//! Detects communities over the knowledge graph (Louvain via
//! `engram-graph-analytics`), builds a deterministic text summary per community,
//! and ranks communities by lexical relevance to the query. Returns the top
//! MATCHING community's member entities as candidates — direct matches first,
//! then graph expansion — with scores derived from the actual match. A query
//! that matches no community summary returns nothing: graph lanes expand from
//! matches, they never substitute for one (GraphRAG, Edge et al. 2024).
//!
//! Engine-neutral; reuses `GraphRelationshipSource` from `engram-store-
//! associative-graph`. No contract change (`RetrievalMode::Graph` is frozen).

mod index;

pub use index::CommunitySummaryIndex;
