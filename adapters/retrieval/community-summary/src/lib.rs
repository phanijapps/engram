//! Community-summary retrieval — a `RetrievalIndex` for `RetrievalMode::Graph`.
//!
//! Detects communities over the knowledge graph (Louvain via
//! `engram-graph-analytics`), builds a deterministic text summary per community,
//! and ranks communities by lexical relevance to the query. Returns the top
//! community's member entities as candidates — global/thematic recall that
//! point-retrieval cannot produce (GraphRAG, Edge et al. 2024).
//!
//! Engine-neutral; reuses `GraphRelationshipSource` from `engram-store-
//! associative-graph`. No contract change (`RetrievalMode::Graph` is frozen).

mod index;

pub use index::CommunitySummaryIndex;
