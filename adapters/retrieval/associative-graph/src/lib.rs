//! Associative graph retrieval — a `RetrievalIndex` for `RetrievalMode::Graph`.
//!
//! Ranks knowledge-graph entities by Personalized PageRank seeded at the
//! entities named in a retrieval query, giving agents associative / multi-hop
//! recall ("what else is connected to what I asked about") beyond what lexical
//! and vector retrieval produce. The crate is engine-neutral: all graph data
//! enters through the injected [`GraphRelationshipSource`] trait, so no SQL or
//! storage-engine type lives here. See
//! `docs/specs/associative-graph-retrieval/`.
//!
//! This crate ships the adapter unit only; composing it into the live retrieval
//! pipeline (a `GraphRelationshipSource` backed by a concrete knowledge store +
//! a bindings/provider seam) is the separate `associative-graph-wiring` slice.

mod index;
mod ranking;
mod seeds;
mod source;

pub use index::AssociativeGraphIndex;
pub use ranking::PprConfig;
pub use source::GraphRelationshipSource;
