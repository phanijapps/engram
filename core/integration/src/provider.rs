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
    batch::BatchIngest, capability::CapabilityReport, config::EngramConfig,
    embedding::EmbeddingProvider, export_import::ExportImport, migration::MigrationService,
    observability::Observability, provenance::ProvenanceQuery, recall::UnifiedRecall,
};

/// Canonical Rust SDK entry point for host applications (`engram-host-sdk`
/// brief). Open one provider from an [`EngramConfig`] and reach every supported
/// engram service — memory, knowledge, graph, ontology, taxonomy, beliefs,
/// hierarchy, retrieval, vectors, migration — through backend-neutral
/// `Arc<dyn ...>` handles, with no engine-specific types in scope.
///
/// Read the [`CapabilityReport`] (18 keys) via [`capabilities`](Self::capabilities)
/// before using a family: each handle is `Some` only when that family's
/// conformance fixture passed during bootstrap; otherwise the family is reported
/// [`CapabilityState::Unsupported`] with a stable reason code and the handle is
/// `None`. The 8 not-yet-built areas are reported
/// `Unsupported { FeatureDisabled }` — present and explicit, never silently
/// absent.
///
/// Hosts select a backend declaratively via config. SQLite is the active
/// backend today; the engine-neutral contract (ADR-0022) keeps a future backend
/// a config/crate change, never an application rewrite.
///
/// See `docs/guides/how-to/use-engram-provider.md` for a host-usage walkthrough.
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
    provenance: Option<Arc<dyn ProvenanceQuery>>,
    batch: Option<Arc<dyn BatchIngest>>,
    recall: Option<Arc<dyn UnifiedRecall>>,
    export_import: Option<Arc<dyn ExportImport>>,
    observability: Option<Arc<dyn Observability>>,
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

    /// Opens a provider from configuration, selecting the backend from the
    /// enabled cargo feature. This is the **sole runtime entry point** for host
    /// applications: open one provider from an [`EngramConfig`] and reach every
    /// supported engram service through the returned facade.
    ///
    /// With the `sqlite` feature enabled (the active backend) this constructs
    /// every file-backed store, gates each capability family on an inlined
    /// conformance check, and returns a fully-wired provider. With no backend
    /// feature enabled it returns [`CoreError::CapabilityUnsupported`] — compile
    /// with `--features sqlite` (or a future backend feature) to obtain a wired
    /// provider.
    ///
    /// This method is **not `async`** even though the trait handles it returns
    /// are async: the SQLite adapters are synchronous rusqlite bodies wrapped in
    /// async-by-convention trait methods, so `open` drives them to completion
    /// inline without a host runtime. If a future backend needs a runtime, the
    /// async variant will live on a backend-specific entry point so this
    /// synchronous contract stays stable for hosts that open the provider from
    /// `main()` before any async context exists.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidRequest`] if configuration validation fails,
    /// or [`CoreError::CapabilityUnsupported`] when no backend feature is
    /// enabled.
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
    ///     let provider = EngramProvider::open(&config)?;
    ///     let report = provider.capabilities();
    ///     Ok(())
    /// }
    /// ```
    pub fn open(config: &EngramConfig) -> CoreResult<Self> {
        config.validate().map_err(|e| CoreError::InvalidRequest {
            reason: format!("configuration validation failed: {e}"),
        })?;

        #[cfg(feature = "sqlite")]
        {
            crate::sqlite::bootstrap_sqlite(config)
        }

        #[cfg(not(any(feature = "sqlite")))]
        {
            let _ = config;
            Err(CoreError::CapabilityUnsupported {
                capability: "backend".to_string(),
                reason: "no backend feature enabled — compile with --features sqlite".to_string(),
            })
        }
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

    /// Returns the provenance / evidence query handle if the
    /// `episodes_evidence` capability is supported.
    pub fn provenance(&self) -> Option<&Arc<dyn ProvenanceQuery>> {
        self.provenance.as_ref()
    }

    /// Returns the best-effort batch ingest handle if the `atomic_batch`
    /// capability is supported.
    pub fn batch(&self) -> Option<&Arc<dyn BatchIngest>> {
        self.batch.as_ref()
    }

    /// Returns the unified recall handle if the `unified_recall` capability is
    /// supported.
    pub fn recall(&self) -> Option<&Arc<dyn UnifiedRecall>> {
        self.recall.as_ref()
    }

    /// Returns the export-import handle if the `export_import` capability is
    /// supported.
    pub fn export_import(&self) -> Option<&Arc<dyn ExportImport>> {
        self.export_import.as_ref()
    }

    /// Returns the observability / diagnostics handle if the `observability`
    /// capability is supported.
    pub fn observability(&self) -> Option<&Arc<dyn Observability>> {
        self.observability.as_ref()
    }

    // ---- require_*: error-on-absent variants for hosts that prefer a typed
    // error over `Option` unwrapping. Each returns `CapabilityUnsupported` when
    // the handle is not wired, naming the capability so callers can branch on a
    // stable reason code rather than parsing the report.

    /// Returns the memory repository handle or an error if it is not wired.
    pub fn require_memory(&self) -> CoreResult<&Arc<dyn MemoryService>> {
        self.memory()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "memory".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the knowledge repository handle or an error if it is not wired.
    pub fn require_knowledge(&self) -> CoreResult<&Arc<dyn KnowledgeRepository>> {
        self.knowledge()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "knowledge".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the knowledge-graph repository handle or an error if it is not wired.
    pub fn require_graph(&self) -> CoreResult<&Arc<dyn KnowledgeGraphRepository>> {
        self.graph()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "graph".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the ontology repository handle or an error if it is not wired.
    pub fn require_ontology(&self) -> CoreResult<&Arc<dyn OntologyRepository>> {
        self.ontology()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "ontology".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the taxonomy repository handle or an error if it is not wired.
    pub fn require_taxonomy(&self) -> CoreResult<&Arc<dyn TaxonomyRepository>> {
        self.taxonomy()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "taxonomy".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the belief repository handle or an error if it is not wired.
    pub fn require_beliefs(&self) -> CoreResult<&Arc<dyn BeliefRepository>> {
        self.beliefs()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "beliefs".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the hierarchy repository handle or an error if it is not wired.
    pub fn require_hierarchy(&self) -> CoreResult<&Arc<dyn HierarchyRepository>> {
        self.hierarchy()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "hierarchy".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the retrieval index handle or an error if it is not wired.
    pub fn require_retrieval(&self) -> CoreResult<&Arc<dyn RetrievalIndex>> {
        self.retrieval()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "retrieval".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the vector index handle or an error if it is not wired.
    pub fn require_vectors(&self) -> CoreResult<&Arc<dyn VectorIndex>> {
        self.vectors()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "vectors".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the migration service handle or an error if it is not wired.
    pub fn require_migration(&self) -> CoreResult<&Arc<dyn MigrationService>> {
        self.migration()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "migration".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the embedding provider or an error if it is not configured.
    pub fn require_embedding_provider(&self) -> CoreResult<&Arc<dyn EmbeddingProvider>> {
        self.embedding_provider()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "embedding_provider".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the provenance / evidence query handle or an error if it is not
    /// wired.
    pub fn require_provenance(&self) -> CoreResult<&Arc<dyn ProvenanceQuery>> {
        self.provenance()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "episodes_evidence".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the best-effort batch ingest handle or an error if it is not
    /// wired.
    pub fn require_batch(&self) -> CoreResult<&Arc<dyn BatchIngest>> {
        self.batch()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "atomic_batch".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the unified recall handle or an error if it is not wired.
    pub fn require_recall(&self) -> CoreResult<&Arc<dyn UnifiedRecall>> {
        self.recall()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "unified_recall".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the export-import handle or an error if it is not wired.
    pub fn require_export_import(&self) -> CoreResult<&Arc<dyn ExportImport>> {
        self.export_import()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "export_import".to_string(),
                reason: "not wired".to_string(),
            })
    }

    /// Returns the observability / diagnostics handle or an error if it is not
    /// wired.
    pub fn require_observability(&self) -> CoreResult<&Arc<dyn Observability>> {
        self.observability()
            .ok_or_else(|| CoreError::CapabilityUnsupported {
                capability: "observability".to_string(),
                reason: "not wired".to_string(),
            })
    }
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
        // Not-yet-built capability areas are reported explicitly as
        // FeatureDisabled (their implementation slices have not shipped), never
        // silently absent. Distinct from the implemented families'
        // ProviderUnavailable (a backend simply is not wired here).
        let not_built = CapabilityState::Unsupported {
            reason: CapabilityReason::FeatureDisabled,
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
                .hybrid_search(not_built.clone())
                .episodes_evidence(not_built.clone())
                .contradiction(not_built.clone())
                .atomic_batch(not_built.clone())
                .unified_recall(not_built.clone())
                .export_import(not_built.clone())
                .maintenance(not_built.clone())
                .observability(not_built)
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
            provenance: None,
            batch: None,
            recall: None,
            export_import: None,
            observability: None,
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
    provenance: Option<Arc<dyn ProvenanceQuery>>,
    batch: Option<Arc<dyn BatchIngest>>,
    recall: Option<Arc<dyn UnifiedRecall>>,
    export_import: Option<Arc<dyn ExportImport>>,
    observability: Option<Arc<dyn Observability>>,
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
            provenance: None,
            batch: None,
            recall: None,
            export_import: None,
            observability: None,
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

    /// Attaches the provenance / evidence query handle.
    pub fn provenance(mut self, handle: Arc<dyn ProvenanceQuery>) -> Self {
        self.provenance = Some(handle);
        self
    }

    /// Attaches the best-effort batch ingest handle.
    pub fn batch(mut self, handle: Arc<dyn BatchIngest>) -> Self {
        self.batch = Some(handle);
        self
    }

    /// Attaches the unified recall handle.
    pub fn recall(mut self, handle: Arc<dyn UnifiedRecall>) -> Self {
        self.recall = Some(handle);
        self
    }

    /// Attaches the export-import handle.
    pub fn export_import(mut self, handle: Arc<dyn ExportImport>) -> Self {
        self.export_import = Some(handle);
        self
    }

    /// Attaches the observability / diagnostics handle.
    pub fn observability(mut self, handle: Arc<dyn Observability>) -> Self {
        self.observability = Some(handle);
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
            provenance: self.provenance,
            batch: self.batch,
            recall: self.recall,
            export_import: self.export_import,
            observability: self.observability,
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
        assert!(
            provider.batch().is_none(),
            "unwired provider has no batch handle"
        );
        assert!(
            provider.recall().is_none(),
            "unwired provider has no recall handle"
        );
        assert!(!provider.capabilities().all_supported());
        assert_eq!(provider.schema_version(), "unwired");
        // AC2: all 8 not-yet-built areas are explicitly Unsupported { FeatureDisabled },
        // present in the report (never silently absent).
        let caps = provider.capabilities();
        let feature_disabled = CapabilityState::Unsupported {
            reason: CapabilityReason::FeatureDisabled,
        };
        for (name, state) in [
            ("hybrid_search", &caps.hybrid_search),
            ("episodes_evidence", &caps.episodes_evidence),
            ("contradiction", &caps.contradiction),
            ("atomic_batch", &caps.atomic_batch),
            ("unified_recall", &caps.unified_recall),
            ("export_import", &caps.export_import),
            ("maintenance", &caps.maintenance),
            ("observability", &caps.observability),
        ] {
            assert_eq!(
                state, &feature_disabled,
                "area `{name}` should be Unsupported {{ FeatureDisabled }}"
            );
        }
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

    #[test]
    fn config_only_bootstrap_has_no_batch_handle() {
        // The config-only EngramProvider::bootstrap (no adapter wired) reports
        // atomic_batch Unsupported { FeatureDisabled } with no handle — the
        // contrast that proves the wiring layer only flips the capability with
        // a handle.
        let provider = EngramProvider::bootstrap(&test_config()).unwrap();
        assert!(
            provider.batch().is_none(),
            "unwired provider has no batch handle"
        );
        assert_eq!(
            provider.capabilities().atomic_batch,
            CapabilityState::Unsupported {
                reason: CapabilityReason::FeatureDisabled,
            },
            "atomic_batch is FeatureDisabled until the batch fixture passes"
        );
    }

    #[test]
    fn builder_attaches_batch_handle() {
        use crate::batch::{
            ALL_STEPS, BatchIngest, BatchIngestRequest, BatchOutcome, StepOutcome, StepStatus,
            TransactionGuarantee, aggregate_status,
        };
        use async_trait::async_trait;

        struct StubBatch;
        #[async_trait]
        impl BatchIngest for StubBatch {
            fn transaction_guarantee(&self) -> TransactionGuarantee {
                TransactionGuarantee::BestEffort
            }
            async fn ingest(
                &self,
                _request: BatchIngestRequest,
            ) -> engram_runtime::CoreResult<BatchOutcome> {
                let steps: Vec<StepOutcome> = ALL_STEPS
                    .iter()
                    .map(|&s| StepOutcome::ok(s, StepStatus::Succeeded))
                    .collect();
                Ok(BatchOutcome {
                    guarantee: TransactionGuarantee::BestEffort,
                    status: aggregate_status(&steps),
                    steps,
                })
            }
        }

        let report = CapabilityReport::builder().build();
        let provider = EngramProviderBuilder::new(report)
            .batch(std::sync::Arc::new(StubBatch))
            .build();
        let handle = provider.batch().expect("batch handle attached");
        assert_eq!(
            handle.transaction_guarantee(),
            TransactionGuarantee::BestEffort
        );
    }

    #[test]
    fn builder_attaches_recall_handle() {
        use crate::recall::UnifiedRecall;
        use async_trait::async_trait;
        use engram_domain::{ContextPayload, RetrievalRequest};

        struct StubRecall;
        #[async_trait]
        impl UnifiedRecall for StubRecall {
            async fn recall(
                &self,
                _request: RetrievalRequest,
            ) -> engram_runtime::CoreResult<ContextPayload> {
                Ok(ContextPayload {
                    items: Vec::new(),
                    budget: None,
                    omitted: Vec::new(),
                    source_failures: Vec::new(),
                    created_at: chrono::Utc::now(),
                })
            }
        }

        let report = CapabilityReport::builder().build();
        let provider = EngramProviderBuilder::new(report)
            .recall(std::sync::Arc::new(StubRecall))
            .build();
        assert!(
            provider.recall().is_some(),
            "recall handle must be attached"
        );
    }

    #[test]
    fn builder_attaches_export_import_handle() {
        use crate::export_import::ExportImport;
        use crate::migration::ImportData;
        use async_trait::async_trait;
        use engram_domain::Scope;

        struct StubExportImport;
        #[async_trait]
        impl ExportImport for StubExportImport {
            async fn export(&self, _scope: &Scope) -> engram_runtime::CoreResult<ImportData> {
                Ok(ImportData {
                    memories: Vec::new(),
                    knowledge_sources: Vec::new(),
                    knowledge_documents: Vec::new(),
                    knowledge_chunks: Vec::new(),
                    knowledge_entities: Vec::new(),
                    knowledge_relationships: Vec::new(),
                    concept_schemes: Vec::new(),
                    concepts: Vec::new(),
                    beliefs: Vec::new(),
                    hierarchy_nodes: Vec::new(),
                    vectors: Vec::new(),
                })
            }
        }

        let report = CapabilityReport::builder().build();
        let provider = EngramProviderBuilder::new(report)
            .export_import(std::sync::Arc::new(StubExportImport))
            .build();
        assert!(
            provider.export_import().is_some(),
            "export_import handle must be attached"
        );
    }

    #[test]
    fn builder_attaches_observability_handle() {
        use crate::config::EmbeddingProviderConfig;
        use crate::observability::{DiagnosticsSnapshot, Observability, RecordCounts};
        use async_trait::async_trait;

        struct StubObservability;
        #[async_trait]
        impl Observability for StubObservability {
            async fn diagnostics(&self) -> engram_runtime::CoreResult<DiagnosticsSnapshot> {
                Ok(DiagnosticsSnapshot {
                    capabilities: CapabilityReport::new(CapabilityState::Supported),
                    record_counts: RecordCounts::empty(),
                    embedding_config: EmbeddingProviderConfig {
                        provider_type: "test".to_string(),
                        model: "test_model".to_string(),
                        dimensions: 384,
                        prompt_profile: "query".to_string(),
                        normalization: None,
                    },
                    schema_version: "2026.01".to_string(),
                    adapter_version: "0.1.0".to_string(),
                    slow_query_diagnostics: None,
                })
            }
        }

        let report = CapabilityReport::builder().build();
        let provider = EngramProviderBuilder::new(report)
            .observability(std::sync::Arc::new(StubObservability))
            .build();
        assert!(
            provider.observability().is_some(),
            "observability handle must be attached"
        );
    }
}
