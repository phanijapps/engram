//! SQLite persistence adapter for source-grounded knowledge, graphs, and taxonomy.
//!
//! This crate stores knowledge sources, documents, chunks, entities,
//! relationships, graphs, concept schemes, concepts, and relations as contract
//! JSON with scope and lookup indexing — the knowledge-domain counterpart to
//! `engram-store-sql`. It implements the `KnowledgeRepository`,
//! `KnowledgeGraphRepository`, and `TaxonomyRepository` ports from
//! `engram-knowledge`.
//!
//! It must not depend on the memory SQL adapter or the vector adapter. Each
//! storage concern stays behind its own crate boundary so a durable knowledge
//! backend can evolve (or move to Postgres / a graph store) without coupling.

mod graph;
mod knowledge;
mod ontology;
mod retrieval;
mod schema;
mod scope;
mod service;
mod taxonomy;

pub use retrieval::{GraphCandidateSource, GraphRetrievalIndex};
pub use service::SqlKnowledgeStore;
