//! Knowledge, graph, ontology, and source-ingestion behavior ports.
//!
//! This crate owns source-grounded knowledge contracts that can be backed by
//! document stores, vector indexes, RDF stores, property graphs, or graph
//! databases without depending on memory persistence.
//!
//! Each port lives in a focused module; this crate root is a facade that
//! re-exports the public surface so downstream crates depend on the names, not
//! the internal module layout.

mod graph;
pub mod identity;
mod ingest;
mod maintenance;
mod ontology;
mod repository;
mod taxonomy;
mod taxonomy_validation;

pub use engram_runtime::{CoreError, CoreResult};

pub use graph::KnowledgeGraphRepository;
pub use identity::EntityIdentityRepository;
pub use ingest::{Chunker, IngestionService, SourceReader};
pub use maintenance::GraphMaintenanceRepository;
/// Pure candidate detection over pre-fetched graph data — the cross-crate utility
/// the SQLite adapter (T4/T5) composes inside its async `detect_candidates`.
pub use maintenance::detect_candidates;
pub use ontology::OntologyRepository;
pub use repository::KnowledgeRepository;
pub use taxonomy::TaxonomyRepository;
pub use taxonomy_validation::validate_taxonomy_proposal;
