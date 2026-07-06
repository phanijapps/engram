//! Engram provider facade for external Rust applications.
//!
//! The provider bundles typed repository handles with explicit capability
//! reporting. It is a thin facade: it holds `Arc<dyn ...>` trait handles and a
//! [`CapabilityReport`], and reports capabilities that a wiring layer (the
//! adapters crate) has already verified against conformance fixtures.
//!
//! `core/integration` deliberately depends only on the port trait crates — it
//! never constructs an adapter. The adapter-construction + fixture-gated
//! capability detection lives in the adapters layer
//! (`engram-conformance::wiring::bootstrap_provider`), which builds a provider
//! through [`EngramProviderBuilder`].

use engram_belief::BeliefRepository;
use engram_domain::{CapabilityReason, CapabilityState};
use engram_hierarchy::HierarchyRepository;
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
};
use engram_memory::MemoryService;
use engram_retrieval::{RetrievalIndex, VectorIndex};
use engram_runtime::{CoreError, CoreResult};
use std::sync::Arc;

use crate::{
    capability::CapabilityReport, config::EngramConfig, embedding::EmbeddingProvider,
    migration::MigrationService,
};

/// Provider facade that bundles repository handles with capability reporting.
///
/// External applications use this provider to access all supported Engram
/// services through a single bootstrap point. Each handle is `Some` only when
/// the corresponding conformance fixture passed during bootstrap; otherwise the
/// family is reported `Unsupported` and the handle is `None`.
pub struct EngramProvider {
    capabilities: CapabilityReport,
    memory: Option<Arc<dyn MemoryService>>,
    knowledge: Option<Arc<dyn KnowledgeRepository>>,
    graph: Option<Arc<dyn KnowledgeGraphRepository>>,
    ontology: Option<Arc<dyn OntologyRepository>>,
    taxonomy: Option<Arc<dyn TaxonomyRepository>>,
    beliefs: Option<Arc<dyn BeliefRepository>>,
    hierarchy: Option<Arc<dyn HierarchyRepository>>,
    retrieval: Option<Arc<dyn RetrievalIndex>>,
    vectors: Option<Arc<dyn VectorIndex>>,
    migration: Option<Arc<dyn MigrationService>>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    schema_version: String,
    adapter_version: String,
}

impl EngramProvider {
    /// Validates configuration and returns a provider with no handles.
    ///
    /// This is the config-validation entry point. Every family is reported
    /// `Unsupported` (`ProviderUnavailable`) because no adapter has been wired.
    /// To obtain a provider with real, fixture-verified handles, use the
    /// adapters-layer `engram_conformance::wiring::bootstrap_provider`, which
    /// constructs the adapters, runs each conformance fixture, and attaches the
    /// handle only when the fixture passes.
    ///
    /// # Errors
    ///
    /// Returns `CoreError::InvalidRequest` if configuration validation fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use engram_integration::{EngramConfig, CapabilityPolicy, MigrationMode, EmbeddingProviderConfig, EngramProvider};
    /// use engram_domain::types::ScopeMappingStrategy;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let config = EngramConfig::new(
    ///         "/var/lib/engram",
    ///         "/var/lib",
    ///         ScopeMappingStrategy::Strict,
    ///         EmbeddingProviderConfig {
    ///             provider_type: "test".to_string(),
    ///             model: "test_model".to_string(),
    ///             dimensions: 384,
    ///             prompt_profile: "query".to_string(),
    ///             normalization: None,
    ///         },
    ///         MigrationMode::DryRun,
    ///         CapabilityPolicy::FailClosed,
    ///     );
    ///
    ///     let provider = EngramProvider::bootstrap(&config)?;
    ///     let report = provider.capabilities();
    ///     Ok(())
    /// }
    /// ```
    pub fn bootstrap(config: &EngramConfig) -> CoreResult<Self> {
        config.validate().map_err(|e| CoreError::InvalidRequest {
            reason: format!("configuration validation failed: {e}"),
        })?;
        Ok(Self::empty())
    }

    /// Returns the capability report for this provider.
    pub fn capabilities(&self) -> &CapabilityReport {
        &self.capabilities
    }

    /// Returns the memory repository handle if the memory capability is supported.
    pub fn memory(&self) -> Option<&Arc<dyn MemoryService>> {
        self.memory.as_ref()
    }

    /// Returns the knowledge repository handle if supported.
    pub fn knowledge(&self) -> Option<&Arc<dyn KnowledgeRepository>> {
        self.knowledge.as_ref()
    }

    /// Returns the knowledge-graph repository handle if supported.
    pub fn graph(&self) -> Option<&Arc<dyn KnowledgeGraphRepository>> {
        self.graph.as_ref()
    }

    /// Returns the ontology repository handle if supported.
    pub fn ontology(&self) -> Option<&Arc<dyn OntologyRepository>> {
        self.ontology.as_ref()
    }

    /// Returns the taxonomy repository handle if supported.
    pub fn taxonomy(&self) -> Option<&Arc<dyn TaxonomyRepository>> {
        self.taxonomy.as_ref()
    }

    /// Returns the belief repository handle if supported.
    pub fn beliefs(&self) -> Option<&Arc<dyn BeliefRepository>> {
        self.beliefs.as_ref()
    }

    /// Returns the hierarchy repository handle if supported.
    pub fn hierarchy(&self) -> Option<&Arc<dyn HierarchyRepository>> {
        self.hierarchy.as_ref()
    }

    /// Returns the retrieval index handle if supported.
    pub fn retrieval(&self) -> Option<&Arc<dyn RetrievalIndex>> {
        self.retrieval.as_ref()
    }

    /// Returns the vector index handle if supported.
    pub fn vectors(&self) -> Option<&Arc<dyn VectorIndex>> {
        self.vectors.as_ref()
    }

    /// Returns the migration service handle if supported.
    pub fn migration(&self) -> Option<&Arc<dyn MigrationService>> {
        self.migration.as_ref()
    }

    /// Returns the embedding provider if configured.
    pub fn embedding_provider(&self) -> Option<&Arc<dyn EmbeddingProvider>> {
        self.embedding_provider.as_ref()
    }

    /// Returns the storage schema version visible through provider diagnostics.
    pub fn schema_version(&self) -> &str {
        &self.schema_version
    }

    /// Returns the adapter version visible through provider diagnostics.
    pub fn adapter_version(&self) -> &str {
        &self.adapter_version
    }

    /// Constructs a provider with no handles and all capabilities unsupported.
    fn empty() -> Self {
        let unavailable = CapabilityState::Unsupported {
            reason: CapabilityReason::ProviderUnavailable,
        };
        Self {
            capabilities: CapabilityReport::builder()
                .memory(unavailable.clone())
                .knowledge(unavailable.clone())
                .graph(unavailable.clone())
                .ontology(unavailable.clone())
                .taxonomy(unavailable.clone())
                .beliefs(unavailable.clone())
                .hierarchy(unavailable.clone())
                .retrieval(unavailable.clone())
                .vectors(unavailable.clone())
                .migration(unavailable)
                .build(),
            memory: None,
            knowledge: None,
            graph: None,
            ontology: None,
            taxonomy: None,
            beliefs: None,
            hierarchy: None,
            retrieval: None,
            vectors: None,
            migration: None,
            embedding_provider: None,
            schema_version: "unwired".to_string(),
            adapter_version: "unwired".to_string(),
        }
    }
}

/// Builder for an [`EngramProvider`] from pre-constructed handles and verified
/// capability states.
///
/// The adapters layer uses this after running each conformance fixture: a
/// handle is attached and the family marked `Supported` only when the fixture
/// passes; otherwise the family is marked `Unsupported` with a reason and the
/// handle left absent.
pub struct EngramProviderBuilder {
    capabilities: CapabilityReport,
    memory: Option<Arc<dyn MemoryService>>,
    knowledge: Option<Arc<dyn KnowledgeRepository>>,
    graph: Option<Arc<dyn KnowledgeGraphRepository>>,
    ontology: Option<Arc<dyn OntologyRepository>>,
    taxonomy: Option<Arc<dyn TaxonomyRepository>>,
    beliefs: Option<Arc<dyn BeliefRepository>>,
    hierarchy: Option<Arc<dyn HierarchyRepository>>,
    retrieval: Option<Arc<dyn RetrievalIndex>>,
    vectors: Option<Arc<dyn VectorIndex>>,
    migration: Option<Arc<dyn MigrationService>>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    schema_version: String,
    adapter_version: String,
}

impl EngramProviderBuilder {
    /// Creates a builder seeded with an existing capability report.
    pub fn new(capabilities: CapabilityReport) -> Self {
        Self {
            capabilities,
            memory: None,
            knowledge: None,
            graph: None,
            ontology: None,
            taxonomy: None,
            beliefs: None,
            hierarchy: None,
            retrieval: None,
            vectors: None,
            migration: None,
            embedding_provider: None,
            schema_version: "unknown".to_string(),
            adapter_version: "unknown".to_string(),
        }
    }

    /// Attaches the memory repository handle.
    pub fn memory(mut self, handle: Arc<dyn MemoryService>) -> Self {
        self.memory = Some(handle);
        self
    }

    /// Attaches the knowledge repository handle.
    pub fn knowledge(mut self, handle: Arc<dyn KnowledgeRepository>) -> Self {
        self.knowledge = Some(handle);
        self
    }

    /// Attaches the knowledge-graph repository handle.
    pub fn graph(mut self, handle: Arc<dyn KnowledgeGraphRepository>) -> Self {
        self.graph = Some(handle);
        self
    }

    /// Attaches the ontology repository handle.
    pub fn ontology(mut self, handle: Arc<dyn OntologyRepository>) -> Self {
        self.ontology = Some(handle);
        self
    }

    /// Attaches the taxonomy repository handle.
    pub fn taxonomy(mut self, handle: Arc<dyn TaxonomyRepository>) -> Self {
        self.taxonomy = Some(handle);
        self
    }

    /// Attaches the belief repository handle.
    pub fn beliefs(mut self, handle: Arc<dyn BeliefRepository>) -> Self {
        self.beliefs = Some(handle);
        self
    }

    /// Attaches the hierarchy repository handle.
    pub fn hierarchy(mut self, handle: Arc<dyn HierarchyRepository>) -> Self {
        self.hierarchy = Some(handle);
        self
    }

    /// Attaches the retrieval index handle.
    pub fn retrieval(mut self, handle: Arc<dyn RetrievalIndex>) -> Self {
        self.retrieval = Some(handle);
        self
    }

    /// Attaches the vector index handle.
    pub fn vectors(mut self, handle: Arc<dyn VectorIndex>) -> Self {
        self.vectors = Some(handle);
        self
    }

    /// Attaches the migration service handle.
    pub fn migration(mut self, handle: Arc<dyn MigrationService>) -> Self {
        self.migration = Some(handle);
        self
    }

    /// Attaches the embedding provider.
    pub fn embedding_provider(mut self, handle: Arc<dyn EmbeddingProvider>) -> Self {
        self.embedding_provider = Some(handle);
        self
    }

    /// Sets the storage schema version reported by provider diagnostics.
    pub fn schema_version(mut self, version: impl Into<String>) -> Self {
        self.schema_version = version.into();
        self
    }

    /// Sets the adapter version reported by provider diagnostics.
    pub fn adapter_version(mut self, version: impl Into<String>) -> Self {
        self.adapter_version = version.into();
        self
    }

    /// Builds the provider.
    pub fn build(self) -> EngramProvider {
        EngramProvider {
            capabilities: self.capabilities,
            memory: self.memory,
            knowledge: self.knowledge,
            graph: self.graph,
            ontology: self.ontology,
            taxonomy: self.taxonomy,
            beliefs: self.beliefs,
            hierarchy: self.hierarchy,
            retrieval: self.retrieval,
            vectors: self.vectors,
            migration: self.migration,
            embedding_provider: self.embedding_provider,
            schema_version: self.schema_version,
            adapter_version: self.adapter_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CapabilityPolicy, EmbeddingProviderConfig, MigrationMode};
    use engram_domain::types::ScopeMappingStrategy;

    fn test_config() -> EngramConfig {
        let temp = std::env::temp_dir();
        EngramConfig::new(
            temp.join("engram-provider-test"),
            temp,
            ScopeMappingStrategy::Strict,
            EmbeddingProviderConfig {
                provider_type: "test".to_string(),
                model: "test_model".to_string(),
                dimensions: 384,
                prompt_profile: "query".to_string(),
                normalization: None,
            },
            MigrationMode::DryRun,
            CapabilityPolicy::FailClosed,
        )
    }

    #[test]
    fn bootstrap_rejects_invalid_config() {
        let bad = EngramConfig::new(
            "",
            "/tmp",
            ScopeMappingStrategy::Strict,
            EmbeddingProviderConfig {
                provider_type: "test".to_string(),
                model: "test_model".to_string(),
                dimensions: 384,
                prompt_profile: "query".to_string(),
                normalization: None,
            },
            MigrationMode::DryRun,
            CapabilityPolicy::FailClosed,
        );
        assert!(EngramProvider::bootstrap(&bad).is_err());
    }

    #[test]
    fn bootstrap_returns_unwired_provider() {
        let provider = EngramProvider::bootstrap(&test_config()).unwrap();
        // No handles wired by the config-only bootstrap.
        assert!(provider.memory().is_none());
        assert!(provider.knowledge().is_none());
        assert!(!provider.capabilities().all_supported());
        assert_eq!(provider.schema_version(), "unwired");
    }

    #[test]
    fn builder_attaches_handles_and_versions() {
        let unavailable = CapabilityState::Unsupported {
            reason: CapabilityReason::ProviderUnavailable,
        };
        let report = CapabilityReport::builder()
            .memory(CapabilityState::Supported)
            .knowledge(unavailable.clone())
            .graph(unavailable.clone())
            .ontology(unavailable.clone())
            .taxonomy(unavailable.clone())
            .beliefs(unavailable.clone())
            .hierarchy(unavailable.clone())
            .retrieval(unavailable.clone())
            .vectors(unavailable.clone())
            .migration(unavailable)
            .build();

        let provider = EngramProviderBuilder::new(report)
            .schema_version("2026.01")
            .adapter_version("0.1.0")
            .build();

        assert_eq!(provider.schema_version(), "2026.01");
        assert_eq!(provider.adapter_version(), "0.1.0");
        // No memory handle was attached even though capability says Supported —
        // the builder does not cross-check; the wiring layer is responsible for
        // only marking Supported when it also attaches the handle.
    }
}
