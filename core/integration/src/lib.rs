//! Engram integration facade for external Rust applications.
//!
//! This crate provides the provider configuration and integration contract
//! that enables external applications to embed the Engram memory engine
//! without adopting Engram's storage layout, runtime policy, or UI assumptions.
//!
//! # Configuration
//!
//! Applications bootstrap the provider with an [`EngramConfig`] that specifies:
//! - Storage path and trusted root for path confinement
//! - Scope policy for authorization enforcement
//! - Embedding provider configuration for vector operations
//! - Migration mode for data import control
//! - Capability policy for unsupported feature handling
//!
//! # Example
//!
//! ```no_run
//! use engram_integration::{EngramConfig, CapabilityPolicy, MigrationMode, EmbeddingProviderConfig};
//! use engram_domain::types::ScopeMappingStrategy;
//!
//! let embedding_config = EmbeddingProviderConfig {
//!     provider_type: "fastembed".to_string(),
//!     model: "BAAI/bge-small-en-v1.5".to_string(),
//!     dimensions: 384,
//!     prompt_profile: "query".to_string(),
//!     normalization: None,
//! };
//!
//! let config = EngramConfig::new(
//!     "/var/lib/engram",
//!     "/var/lib",
//!     ScopeMappingStrategy::Strict,
//!     embedding_config,
//!     MigrationMode::DryRun,
//!     CapabilityPolicy::FailClosed,
//! );
//! ```

pub mod capability;
pub mod config;
pub mod embedding;
pub mod migration;
#[cfg(feature = "ollama")]
pub mod ollama_provider;
pub mod provider;

pub use capability::{CapabilityReport, CapabilityReportBuilder};
pub use config::{CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, MigrationMode};
pub use embedding::EmbeddingProvider;
pub use migration::{
    compute_manifest_fingerprint, record_key_hash, BeliefImportRecord, ConceptImportRecord,
    ConceptSchemeImportRecord, EmbeddingSpaceValidation, HierarchyNodeImportRecord, ImportData,
    KnowledgeChunkImportRecord, KnowledgeDocumentImportRecord, KnowledgeEntityImportRecord,
    KnowledgeRelationshipImportRecord, KnowledgeSourceImportRecord, MemoryImportRecord,
    MigrationManifest, MigrationService, RowCounts, ScopeTranslationFailure,
    ScopeTranslationReport, UnsupportedMapping, ValidationReport, VectorImportRecord,
};
pub use provider::{EngramProvider, EngramProviderBuilder};
