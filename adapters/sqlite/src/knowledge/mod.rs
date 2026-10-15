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

mod candidate_text;
mod code_segments;
mod code_segments_schema;
mod code_segments_state;
mod code_segments_validation;
mod code_source;
mod connection_options;
mod document_code_head;
mod document_derived_write;
mod document_head;
mod document_head_legacy;
mod document_head_records;
mod document_head_state;
mod graph;
mod guarded_source_write;
mod identity;
#[allow(clippy::module_inception)]
mod knowledge;
mod maintenance;
mod ontology;
mod record_writes;
mod retrieval;
mod schema;
mod scope;
mod service;
mod source_access;
mod source_inventory;
mod source_legacy;
mod source_schema;
mod taxonomy;

pub use identity::SqlIdentityStore;
pub use retrieval::{GraphCandidateSource, GraphRetrievalIndex};
pub use service::SqlKnowledgeStore;
