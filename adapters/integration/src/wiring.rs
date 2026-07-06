//! Provider wiring: construct adapters from configuration, run each
//! conformance fixture, and build a fully-wired [`EngramProvider`].
//!
//! This is the boundary-respecting counterpart to `core/integration`'s thin
//! facade: adapter construction and fixture-gated capability detection live
//! here (in the adapters crate), while the provider struct and trait handles
//! live in the port-only core crate.

use std::sync::Arc;

use engram_belief::BeliefRepository;
use engram_domain::{CapabilityReason, CapabilityState};
use engram_hierarchy::HierarchyRepository;
use engram_integration::{CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder};
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
};
use engram_memory::MemoryService;
use engram_runtime::{CoreError, CoreResult};
use engram_store_belief_sqlite::SqlBeliefStore;
use engram_store_hierarchy_sqlite::SqlHierarchyStore;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use engram_store_sql::SqlMemoryService;

use crate::fixtures;

/// Storage schema version reported by provider diagnostics.
const SCHEMA_VERSION: &str = "2026.01";

/// Adapter version reported by provider diagnostics.
const ADAPTER_VERSION: &str = "0.1.0";

/// Bootstraps a fully-wired provider from configuration.
///
/// For each capability family this constructs the SQLite adapter at
/// `<storage_path>/<family>.db`, runs the corresponding conformance fixture,
/// and attaches the handle + marks the family `Supported` only when the
/// fixture passes. A family whose fixture fails is reported `Unsupported`
/// (`ConformanceFailed`) with no handle, so callers can never reach a broken
/// adapter through the facade.
///
/// # Errors
///
/// Returns `CoreError::InvalidRequest` if configuration validation fails.
pub fn bootstrap_provider(config: &EngramConfig) -> CoreResult<EngramProvider> {
    config.validate().map_err(|e| CoreError::InvalidRequest {
        reason: format!("configuration validation failed: {e}"),
    })?;

    let storage = &config.storage_path;
    std::fs::create_dir_all(storage).map_err(|e| CoreError::Adapter {
        adapter: "engram-conformance.wiring".to_string(),
        message: format!("create storage dir: {e}"),
    })?;

    let failed = || CapabilityState::Unsupported {
        reason: CapabilityReason::ConformanceFailed,
    };
    let mut memory_state = failed();
    let mut knowledge_state = failed();
    let mut graph_state = failed();
    let mut ontology_state = failed();
    let mut taxonomy_state = failed();
    let mut beliefs_state = failed();
    let mut hierarchy_state = failed();
    // retrieval_state is set unconditionally below (no RetrievalIndex adapter is
    // wired in this layer); no initial value needed.
    let retrieval_state;
    let mut vectors_state = failed();

    let mut memory: Option<Arc<dyn MemoryService>> = None;
    let mut knowledge: Option<Arc<dyn KnowledgeRepository>> = None;
    let mut graph: Option<Arc<dyn KnowledgeGraphRepository>> = None;
    let mut ontology: Option<Arc<dyn OntologyRepository>> = None;
    let mut taxonomy: Option<Arc<dyn TaxonomyRepository>> = None;
    let mut beliefs: Option<Arc<dyn BeliefRepository>> = None;
    let mut hierarchy: Option<Arc<dyn HierarchyRepository>> = None;
    let retrieval: Option<Arc<dyn engram_retrieval::RetrievalIndex>> = None;
    let mut vectors: Option<Arc<dyn engram_retrieval::VectorIndex>> = None;

    // Memory: run the fixture (capability conformance), then attach a durable
    // file-backed handle.
    if fixtures::memory::run_memory_fixture().is_ok() {
        let path = storage.join("memory.db");
        if let Ok(svc) = SqlMemoryService::open_file(&path) {
            memory = Some(Arc::new(svc));
            memory_state = CapabilityState::Supported;
        }
    }

    // Knowledge + graph + ontology + taxonomy share one SqlKnowledgeStore.
    let knowledge_ok = fixtures::knowledge::run_knowledge_fixture().is_ok();
    let graph_ok = fixtures::knowledge::run_graph_fixture().is_ok();
    let ontology_ok = fixtures::knowledge::run_ontology_fixture().is_ok();
    let taxonomy_ok = fixtures::knowledge::run_taxonomy_fixture().is_ok();
    if knowledge_ok || graph_ok || ontology_ok || taxonomy_ok {
        let path = storage.join("knowledge.db");
        if let Ok(store) = SqlKnowledgeStore::open_file(&path) {
            let store: Arc<SqlKnowledgeStore> = Arc::new(store);
            if knowledge_ok {
                knowledge = Some(store.clone());
                knowledge_state = CapabilityState::Supported;
            }
            if graph_ok {
                graph = Some(store.clone());
                graph_state = CapabilityState::Supported;
            }
            if ontology_ok {
                ontology = Some(store.clone());
                ontology_state = CapabilityState::Supported;
            }
            if taxonomy_ok {
                taxonomy = Some(store.clone());
                taxonomy_state = CapabilityState::Supported;
            }
        }
    }

    // Beliefs.
    if fixtures::belief::run_belief_fixture().is_ok() {
        let path = storage.join("belief.db");
        if let Ok(store) = SqlBeliefStore::open_file(&path) {
            beliefs = Some(Arc::new(store));
            beliefs_state = CapabilityState::Supported;
        }
    }

    // Hierarchy.
    if fixtures::hierarchy::run_hierarchy_fixture().is_ok() {
        let path = storage.join("hierarchy.db");
        if let Ok(store) = SqlHierarchyStore::open_file(&path) {
            hierarchy = Some(Arc::new(store));
            hierarchy_state = CapabilityState::Supported;
        }
    }

    // Vectors: construct a file-backed SqliteVectorIndex configured with the
    // embedding space from configuration, then attach it. The fixture proves
    // the VectorIndex contract; the attached index is the usable instance.
    if fixtures::vector::run_vector_fixture().is_ok() {
        let dims = config.embedding_provider.dimensions;
        let path = storage.join("vectors.db");
        let space = engram_domain::EmbeddingSpace::new(
            &config.embedding_provider.provider_type,
            &config.embedding_provider.model,
            dims,
            &config.embedding_provider.prompt_profile,
            config.embedding_provider.normalization.clone(),
        );
        if let Ok(path_str) = path.to_str().ok_or_else(|| CoreError::InvalidRequest {
            reason: "vector db path is not valid unicode".to_string(),
        }) {
            if let Ok(index) =
                engram_store_vector::SqliteVectorIndex::open_with_embedding_space(path_str, space)
            {
                if index.requires_reindex() {
                    // The index was built under a different embedding space than
                    // the configuration requests: existing vectors are
                    // incompatible and must be rebuilt before use.
                    vectors_state = CapabilityState::RequiresReindex {
                        reason: CapabilityReason::EmbeddingSpaceMismatch,
                    };
                } else {
                    vectors = Some(Arc::new(index));
                    vectors_state = CapabilityState::Supported;
                }
            }
        }
    }
    // Retrieval: the fixture proves the trace contract, but no RetrievalIndex
    // adapter (context composer) is constructed here — that lives in the
    // orchestration layer. Report Unsupported honestly rather than claiming
    // Supported without a handle, so callers never expect a handle that is
    // absent.
    let _ = fixtures::retrieval::run_retrieval_fixture();
    retrieval_state = CapabilityState::Unsupported {
        reason: CapabilityReason::UnsupportedStoreFamily,
    };

    // Migration: the fingerprint contract is verified by the fixture, and a
    // real SqlMigrationService handle is attached so callers can dry-run and
    // apply imports through the facade.
    let mut migration: Option<Arc<dyn engram_integration::MigrationService>> = None;
    let migration_state = if fixtures::migration::run_migration_fixture().is_ok() {
        let svc = crate::migration_service::SqlMigrationService::new(
            config.embedding_provider.dimensions,
        );
        migration = Some(Arc::new(svc));
        CapabilityState::Supported
    } else {
        failed()
    };

    let report = CapabilityReport::builder()
        .memory(memory_state)
        .knowledge(knowledge_state)
        .graph(graph_state)
        .ontology(ontology_state)
        .taxonomy(taxonomy_state)
        .beliefs(beliefs_state)
        .hierarchy(hierarchy_state)
        .retrieval(retrieval_state)
        .vectors(vectors_state)
        .migration(migration_state)
        .build();

    let mut builder = EngramProviderBuilder::new(report)
        .schema_version(SCHEMA_VERSION)
        .adapter_version(ADAPTER_VERSION);
    if let Some(h) = memory {
        builder = builder.memory(h);
    }
    if let Some(h) = knowledge {
        builder = builder.knowledge(h);
    }
    if let Some(h) = graph {
        builder = builder.graph(h);
    }
    if let Some(h) = ontology {
        builder = builder.ontology(h);
    }
    if let Some(h) = taxonomy {
        builder = builder.taxonomy(h);
    }
    if let Some(h) = beliefs {
        builder = builder.beliefs(h);
    }
    if let Some(h) = hierarchy {
        builder = builder.hierarchy(h);
    }
    if let Some(h) = retrieval {
        builder = builder.retrieval(h);
    }
    if let Some(h) = vectors {
        builder = builder.vectors(h);
    }
    if let Some(h) = migration {
        builder = builder.migration(h);
    }
    Ok(builder.build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::types::ScopeMappingStrategy;
    use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, MigrationMode};

    fn cfg() -> EngramConfig {
        let dir = std::env::temp_dir().join(format!("engram-wiring-test-{}", std::process::id()));
        EngramConfig::new(
            dir,
            std::env::temp_dir(),
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
    fn bootstrap_provider_wires_supported_families() {
        let provider = bootstrap_provider(&cfg()).expect("bootstrap");
        let report = provider.capabilities();
        assert!(
            report.memory.is_supported(),
            "memory should be supported: {:?}",
            report.memory
        );
        assert!(report.knowledge.is_supported());
        assert!(report.graph.is_supported());
        assert!(report.ontology.is_supported());
        assert!(report.taxonomy.is_supported());
        assert!(report.beliefs.is_supported());
        assert!(report.hierarchy.is_supported());
        assert!(report.vectors.is_supported());
        assert!(report.migration.is_supported());
        // Retrieval has no adapter wired in this layer: must NOT claim Supported.
        assert!(
            !report.retrieval.is_supported(),
            "retrieval must not be Supported without a handle: {:?}",
            report.retrieval
        );
        // Every Supported repository family has an attached handle.
        assert!(provider.memory().is_some());
        assert!(provider.knowledge().is_some());
        assert!(provider.graph().is_some());
        assert!(provider.ontology().is_some());
        assert!(provider.taxonomy().is_some());
        assert!(provider.beliefs().is_some());
        assert!(provider.hierarchy().is_some());
        assert!(provider.vectors().is_some());
        assert!(provider.migration().is_some());
        // Retrieval handle is absent because no composer is wired here.
        assert!(provider.retrieval().is_none());
        assert_eq!(provider.schema_version(), SCHEMA_VERSION);
    }

    #[test]
    fn supported_family_always_carries_a_handle() {
        // Invariant: no family reported Supported may have a None handle.
        let provider = bootstrap_provider(&cfg()).expect("bootstrap");
        let report = provider.capabilities();
        if report.memory.is_supported() {
            assert!(provider.memory().is_some());
        }
        if report.knowledge.is_supported() {
            assert!(provider.knowledge().is_some());
        }
        if report.vectors.is_supported() {
            assert!(provider.vectors().is_some());
        }
        if report.migration.is_supported() {
            assert!(provider.migration().is_some());
        }
        // Retrieval is the documented exception: Unsupported here, no handle.
        assert!(!report.retrieval.is_supported());
        assert!(provider.retrieval().is_none());
        let _ = report; // silence unused on partial-failure builds
    }
}
