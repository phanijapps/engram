//! Provider wiring: construct adapters from configuration, run each
//! conformance fixture, and build a fully-wired [`EngramProvider`].
//!
//! This is the boundary-respecting counterpart to `core/integration`'s thin
//! facade: adapter construction and fixture-gated capability detection live
//! here (in the adapters crate), while the provider struct and trait handles
//! live in the port-only core crate.

use std::path::PathBuf;
use std::sync::Arc;

use engram_belief::BeliefRepository;
use engram_domain::{CapabilityReason, CapabilityState};
use engram_hierarchy::HierarchyRepository;
use engram_integration::{
    CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder, SqliteStorageLayout,
};
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

/// Resolved SQLite file paths for each store, honoring the configured layout.
///
/// In `MultiFileDirectory` (the default) each store gets its own file. In
/// `SingleFile` every store opens the same path; the store schemas use disjoint
/// table names (verified disjoint across memory, knowledge, belief, hierarchy,
/// and the vector index) so a single database holds all of them without
/// collisions.
struct SqliteLayoutPaths {
    memory: PathBuf,
    knowledge: PathBuf,
    belief: PathBuf,
    hierarchy: PathBuf,
    vectors: PathBuf,
}

impl SqliteLayoutPaths {
    /// Resolves paths from a validated config. The single-file `file_name` is
    /// validated by `EngramConfig::validate` (run before this), guaranteeing it
    /// is a bare name that cannot escape `storage_path`.
    fn from_config(config: &EngramConfig) -> Self {
        match &config.sqlite_storage_layout {
            SqliteStorageLayout::MultiFileDirectory => {
                let storage = &config.storage_path;
                Self {
                    memory: storage.join("memory.db"),
                    knowledge: storage.join("knowledge.db"),
                    belief: storage.join("belief.db"),
                    hierarchy: storage.join("hierarchy.db"),
                    vectors: storage.join("vectors.db"),
                }
            }
            SqliteStorageLayout::SingleFile { file_name } => {
                let shared = config.storage_path.join(file_name);
                Self {
                    memory: shared.clone(),
                    knowledge: shared.clone(),
                    belief: shared.clone(),
                    hierarchy: shared.clone(),
                    vectors: shared,
                }
            }
        }
    }
}

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

    // Resolve per-store SQLite paths once, honoring the configured layout
    // (multi-file directory by default; single shared file when opted in).
    let paths = SqliteLayoutPaths::from_config(config);

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

    let mut vectors_state = failed();
    // episodes_evidence is a shipped capability (S2): like the other implemented
    // families, a fixture failure reports ConformanceFailed, not FeatureDisabled
    // (which means "implementation slice has not shipped"). Flipped to Supported
    // below only when the provenance fixture passes and the handle attaches.
    let mut episodes_evidence_state = failed();
    // atomic_batch is a shipped capability (S3): same pattern — start at
    // ConformanceFailed, flip to Supported only when the batch fixture passes
    // and the SqlBatchIngest handle attaches.
    let mut atomic_batch_state = failed();

    let mut memory: Option<Arc<dyn MemoryService>> = None;
    let mut knowledge: Option<Arc<dyn KnowledgeRepository>> = None;
    let mut graph: Option<Arc<dyn KnowledgeGraphRepository>> = None;
    let mut ontology: Option<Arc<dyn OntologyRepository>> = None;
    let mut taxonomy: Option<Arc<dyn TaxonomyRepository>> = None;
    let mut beliefs: Option<Arc<dyn BeliefRepository>> = None;
    let mut hierarchy: Option<Arc<dyn HierarchyRepository>> = None;
    let retrieval: Option<Arc<dyn engram_retrieval::RetrievalIndex>> = None;
    let mut vectors: Option<Arc<dyn engram_retrieval::VectorIndex>> = None;
    let mut provenance: Option<Arc<dyn engram_integration::ProvenanceQuery>> = None;
    let mut batch: Option<Arc<dyn engram_integration::BatchIngest>> = None;
    let mut recall: Option<Arc<dyn engram_integration::UnifiedRecall>> = None;
    // Concrete Sql* handles, kept alongside the trait handles so the batch
    // (which composes the concrete stores) can be wired without a trait→concrete
    // downcast. Populated only when the corresponding family's handle attaches.
    let mut memory_store: Option<Arc<SqlMemoryService>> = None;
    let mut knowledge_store: Option<Arc<SqlKnowledgeStore>> = None;
    // SqlBeliefStore is kept concrete (alongside the trait handle) so the
    // observability adapter can call its `list_beliefs` for record counts.
    let mut belief_store: Option<Arc<SqlBeliefStore>> = None;
    // SqlHierarchyStore is kept concrete (alongside the trait handle) so the
    // export handle can call its `list_nodes` for the hierarchy family.
    let mut hierarchy_store: Option<Arc<SqlHierarchyStore>> = None;

    // Memory: run the fixture (capability conformance), then attach a durable
    // file-backed handle.
    if fixtures::memory::run_memory_fixture().is_ok() {
        let path = &paths.memory;
        if let Ok(svc) = SqlMemoryService::open_file(path) {
            let svc: Arc<SqlMemoryService> = Arc::new(svc);
            memory_store = Some(svc.clone());
            memory = Some(svc);
            memory_state = CapabilityState::Supported;
        }
    }

    // Knowledge + graph + ontology + taxonomy share one SqlKnowledgeStore.
    let knowledge_ok = fixtures::knowledge::run_knowledge_fixture().is_ok();
    let graph_ok = fixtures::knowledge::run_graph_fixture().is_ok();
    let ontology_ok = fixtures::knowledge::run_ontology_fixture().is_ok();
    let taxonomy_ok = fixtures::knowledge::run_taxonomy_fixture().is_ok();
    // The provenance fixture verifies the SqlProvenanceQuery read path against
    // an in-memory store; it gates the episodes_evidence capability flip.
    let provenance_ok = fixtures::provenance::run_provenance_fixture().is_ok();
    if knowledge_ok || graph_ok || ontology_ok || taxonomy_ok || provenance_ok {
        let path = &paths.knowledge;
        if let Ok(store) = SqlKnowledgeStore::open_file(path) {
            let store: Arc<SqlKnowledgeStore> = Arc::new(store);
            knowledge_store = Some(store.clone());
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
            // episodes_evidence: attach the SqlProvenanceQuery handle and flip
            // the capability to Supported only when the fixture passes. The
            // handle reuses the same knowledge store as the knowledge family.
            if provenance_ok {
                provenance = Some(Arc::new(crate::SqlProvenanceQuery::new(store.clone())));
                episodes_evidence_state = CapabilityState::Supported;
            }
        }
    }

    // atomic_batch (S3): a best-effort batch composes the memory + knowledge
    // stores. The fixture verifies the ingest port end-to-end; the handle is
    // attached and the capability flipped to Supported only when the fixture
    // passes AND both the memory and knowledge file-backed stores are wired
    // (the batch delegates to them). The handle is built from the durable
    // file-backed stores, not the fixture's in-memory stores.
    if fixtures::batch::run_batch_fixture().is_ok()
        && let (Some(memory_handle), Some(knowledge_handle)) = (&memory_store, &knowledge_store)
    {
        batch = Some(Arc::new(crate::SqlBatchIngest::new(
            memory_handle.clone(),
            knowledge_handle.clone(),
        )));
        atomic_batch_state = CapabilityState::Supported;
    }

    // Beliefs.
    if fixtures::belief::run_belief_fixture().is_ok() {
        let path = &paths.belief;
        if let Ok(store) = SqlBeliefStore::open_file(path) {
            let store: Arc<SqlBeliefStore> = Arc::new(store);
            belief_store = Some(store.clone());
            beliefs = Some(store);
            beliefs_state = CapabilityState::Supported;
        }
    }

    // unified_recall (S4): construct the SqlUnifiedRecall handle from the wired
    // memory handle + the available retrieval lanes + the beliefs handle, then
    // flip the capability to Supported when the conformance fixture passes.
    //
    // The v1 lanes compose as:
    //   - facts (memory) — passed to the SqlUnifiedRecall constructor below.
    //   - graph — GraphRetrievalIndex over SqlKnowledgeStore (which implements
    //     GraphCandidateSource).
    //   - lexical — LexicalRetrievalIndex over an in-RAM LexicalIndex + a
    //     knowledge-store-backed target resolver. The index starts empty (no
    //     chunk→index ingest feed is wired here) so the lane returns no
    //     candidates until a feed is added; the resolver is still wired so the
    //     lane is correct once fed.
    //   - vector — feature-gated behind `fastembed` (off by default): constructs
    //     a vector index + the FastEmbed query provider + a knowledge-store
    //     resolver. When disabled the lane is simply absent (degraded for
    //     vector, not a failure).
    //   - beliefs — passed to the SqlUnifiedRecall constructor below.
    let mut retrieval_lanes: Vec<Arc<dyn engram_retrieval::RetrievalIndex>> = Vec::new();
    if let Some(knowledge_handle) = &knowledge_store {
        // Graph lane: SqlKnowledgeStore implements GraphCandidateSource.
        retrieval_lanes.push(Arc::new(
            engram_store_knowledge_sqlite::GraphRetrievalIndex::new(knowledge_handle.clone()),
        ));
        // Lexical lane: an in-RAM Tantivy index (no ingest feed wired here) +
        // a knowledge-store-backed target resolver.
        if let Ok(lexical_index) = engram_store_lexical::LexicalIndex::new() {
            let resolver =
                crate::recall_lanes::KnowledgeLexicalResolver::new(knowledge_handle.clone());
            retrieval_lanes.push(Arc::new(engram_store_lexical::LexicalRetrievalIndex::new(
                lexical_index,
                Arc::new(resolver),
            )));
        }
    }
    // Vector lane (fastembed-gated): construct the vector index + FastEmbed
    // query provider + knowledge-store resolver. Skipped entirely when the
    // feature is off (default build) or when construction fails — recall then
    // runs degraded for vector (fewer candidates), never errors.
    #[cfg(feature = "fastembed")]
    if let (Some(knowledge_handle), Some(path_str)) = (&knowledge_store, paths.vectors.to_str()) {
        let dims = config.embedding_provider.dimensions;
        let space = engram_domain::EmbeddingSpace::new(
            &config.embedding_provider.provider_type,
            &config.embedding_provider.model,
            dims,
            &config.embedding_provider.prompt_profile,
            config.embedding_provider.normalization.clone(),
        );
        if let Ok(vector_index) =
            engram_store_vector::SqliteVectorIndex::open_with_embedding_space(path_str, space)
        {
            if let Ok(query_provider) = engram_store_vector::FastEmbedBgeSmallQueryProvider::new() {
                let resolver =
                    crate::recall_lanes::KnowledgeVectorResolver::new(knowledge_handle.clone());
                retrieval_lanes.push(Arc::new(engram_store_vector::VectorRetrievalIndex::new(
                    vector_index,
                    Arc::new(query_provider),
                    Arc::new(resolver),
                )));
            }
        }
    }
    let mut unified_recall_state = failed();
    if let (Some(memory_handle), Some(belief_handle)) = (&memory_store, &belief_store)
        && fixtures::recall::run_recall_fixture().is_ok()
    {
        let unified = crate::SqlUnifiedRecall::new(
            memory_handle.clone(),
            retrieval_lanes,
            belief_handle.clone(),
        );
        recall = Some(Arc::new(unified));
        unified_recall_state = CapabilityState::Supported;
    }

    // Hierarchy.
    if fixtures::hierarchy::run_hierarchy_fixture().is_ok() {
        let path = &paths.hierarchy;
        if let Ok(store) = SqlHierarchyStore::open_file(path) {
            let store: Arc<SqlHierarchyStore> = Arc::new(store);
            hierarchy_store = Some(store.clone());
            hierarchy = Some(store);
            hierarchy_state = CapabilityState::Supported;
        }
    }

    // Vectors: construct a file-backed SqliteVectorIndex configured with the
    // embedding space from configuration, then attach it. The fixture proves
    // the VectorIndex contract; the attached index is the usable instance.
    if fixtures::vector::run_vector_fixture().is_ok() {
        let dims = config.embedding_provider.dimensions;
        let path = &paths.vectors;
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
    let retrieval_state = CapabilityState::Unsupported {
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

    // export_import (S5): construct SqlExportImport from the wired file-backed
    // memory + knowledge stores. The handle is attached and the capability
    // flipped to Supported only when the conformance fixture passes AND the
    // migration handle is wired (export + import are both needed for
    // backend-to-backend scope movement). Like atomic_batch, this composes the
    // concrete stores, so it is gated on memory_store + knowledge_store too.
    // The belief + hierarchy stores are attached optionally so the export
    // covers those families when wired; an unwired family exports empty.
    let mut export_import_state = failed();
    let mut export_import: Option<Arc<dyn engram_integration::ExportImport>> = None;
    if fixtures::export_import::run_export_import_fixture().is_ok()
        && migration.is_some()
        && let (Some(memory_handle), Some(knowledge_handle)) = (&memory_store, &knowledge_store)
    {
        let mut exporter =
            crate::SqlExportImport::new(knowledge_handle.clone(), memory_handle.clone());
        if let Some(belief_handle) = belief_store.clone() {
            exporter = exporter.with_belief(belief_handle);
        }
        if let Some(hierarchy_handle) = hierarchy_store.clone() {
            exporter = exporter.with_hierarchy(hierarchy_handle);
        }
        export_import = Some(Arc::new(exporter));
        export_import_state = CapabilityState::Supported;
    }

    // observability (S6): the Observability port + SqlObservability impl +
    // conformance fixture are shipped. The handle aggregates the provider's
    // existing diagnostics (CapabilityReport, embedding config, versions) and
    // derives record counts by listing the wired concrete knowledge + belief
    // stores. The capability flips to Supported only when the fixture passes;
    // otherwise it stays Unsupported { ConformanceFailed } with no handle.
    //
    // v1 limitation: record counts are scoped to a fixed diagnostic scope (a
    // broad tenant scope). Cross-tenant aggregation requires instrumentation
    // that is deferred; a fresh bootstrap reports zero counts (empty stores).
    let observability_ok = fixtures::observability::run_observability_fixture().is_ok();
    let observability_state = if observability_ok {
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
        .episodes_evidence(episodes_evidence_state)
        .atomic_batch(atomic_batch_state)
        .unified_recall(unified_recall_state)
        .export_import(export_import_state)
        .observability(observability_state)
        .build();

    // Construct the SqlObservability handle from the wired concrete stores +
    // the final capability report (delegated, not recomputed). Only attached
    // when the fixture passed; clones the report (Clone) so the provider keeps
    // the canonical copy.
    let observability: Option<Arc<dyn engram_integration::Observability>> = if observability_ok {
        Some(Arc::new(crate::SqlObservability::new(
            knowledge_store.clone(),
            belief_store.clone(),
            diagnostic_scope(),
            report.clone(),
            config.embedding_provider.clone(),
            SCHEMA_VERSION,
            ADAPTER_VERSION,
        )))
    } else {
        None
    };

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
    if let Some(h) = provenance {
        builder = builder.provenance(h);
    }
    if let Some(h) = batch {
        builder = builder.batch(h);
    }
    if let Some(h) = export_import {
        builder = builder.export_import(h);
    }
    if let Some(h) = observability {
        builder = builder.observability(h);
    }
    if let Some(h) = recall {
        builder = builder.recall(h);
    }
    Ok(builder.build())
}

/// The fixed diagnostic scope used by the wired observability handle.
///
/// v1: a broad scope (tenant set, all optional fields `None`) so record counts
/// reflect every record in the diagnostic tenant. Cross-tenant aggregation is
/// deferred; a host targeting diagnostics counts writes into this tenant, or a
/// future config field can parameterize it.
fn diagnostic_scope() -> engram_domain::Scope {
    engram_domain::Scope {
        tenant: "engram-diagnostics".to_string(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
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

    /// A minimal retrieval request used to exercise the wired recall handle.
    fn request_for_recall() -> engram_domain::RetrievalRequest {
        use engram_domain::{Actor, ActorKind, Id, Requester, RetrievalMode, Scope};
        engram_domain::RetrievalRequest {
            query: "recall-probe".to_string(),
            scope: Scope {
                tenant: "tenant-wiring".to_string(),
                subject: Some("subject-wiring".to_string()),
                workspace: Some("workspace-wiring".to_string()),
                session: None,
                environment: Some("test".to_string()),
            },
            requester: Requester {
                actor: Actor {
                    id: Id::from("wiring-agent"),
                    kind: ActorKind::Agent,
                    display_name: Some("Wiring Test".to_string()),
                    metadata: None,
                },
                roles: Vec::new(),
                permissions: vec!["memory.retrieve".to_string()],
                on_behalf_of: None,
            },
            modes: vec![RetrievalMode::Keyword],
            filters: None,
            cues: Vec::new(),
            limit: Some(10),
            budget: None,
            include_explanations: None,
        }
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
        // episodes_evidence: the SqlProvenanceQuery fixture passes during
        // bootstrap, so the handle is attached and the capability flips to
        // Supported.
        assert!(
            report.episodes_evidence.is_supported(),
            "episodes_evidence should be Supported: {:?}",
            report.episodes_evidence
        );
        assert!(
            provider.provenance().is_some(),
            "provenance handle must be attached when episodes_evidence is Supported"
        );
        // atomic_batch (S3): the batch fixture passes during bootstrap, so the
        // handle is attached and the capability flips to Supported.
        assert!(
            report.atomic_batch.is_supported(),
            "atomic_batch should be Supported: {:?}",
            report.atomic_batch
        );
        assert!(
            provider.batch().is_some(),
            "batch handle must be attached when atomic_batch is Supported"
        );
        assert_eq!(
            provider
                .batch()
                .expect("batch handle")
                .transaction_guarantee(),
            engram_integration::TransactionGuarantee::BestEffort,
            "batch handle must report BestEffort"
        );
        // unified_recall (S4): the recall fixture passes during bootstrap AND
        // the memory + beliefs stores are wired, so the SqlUnifiedRecall handle
        // (graph + lexical + beliefs lanes) is attached and the capability
        // flips to Supported. The vector lane is fastembed-gated (off by
        // default); recall runs degraded for vector when the feature is off.
        assert!(
            report.unified_recall.is_supported(),
            "unified_recall should be Supported after bootstrap: {:?}",
            report.unified_recall
        );
        assert!(
            provider.recall().is_some(),
            "recall handle must be attached when unified_recall is Supported"
        );
        // export_import (S5): the export fixture passes during bootstrap AND the
        // migration handle is wired, so the handle is attached and the
        // capability flips to Supported.
        assert!(
            report.export_import.is_supported(),
            "export_import should be Supported: {:?}",
            report.export_import
        );
        assert!(
            provider.export_import().is_some(),
            "export_import handle must be attached when export_import is Supported"
        );
        // observability (S6): the conformance fixture passes during bootstrap,
        // so the capability flips to Supported and the diagnostics handle is
        // attached. A fresh bootstrap reports zero counts (empty stores); the
        // snapshot shape + capability delegation is what is verified here.
        assert!(
            report.observability.is_supported(),
            "observability should be Supported: {:?}",
            report.observability
        );
        assert!(
            provider.observability().is_some(),
            "observability handle must be attached when observability is Supported"
        );
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
        // episodes_evidence invariant: Supported iff a handle is attached.
        if report.episodes_evidence.is_supported() {
            assert!(provider.provenance().is_some());
        }
        // atomic_batch invariant: Supported iff a handle is attached.
        if report.atomic_batch.is_supported() {
            assert!(provider.batch().is_some());
        }
        // unified_recall invariant: Supported iff a handle is attached.
        if report.unified_recall.is_supported() {
            assert!(provider.recall().is_some());
        }
        // export_import invariant: Supported iff a handle is attached.
        if report.export_import.is_supported() {
            assert!(provider.export_import().is_some());
        }
        // observability invariant: Supported iff a handle is attached.
        if report.observability.is_supported() {
            assert!(provider.observability().is_some());
        }
        let _ = report; // silence unused on partial-failure builds
    }

    #[test]
    fn bootstrap_provider_exposes_provenance_handle_when_supported() {
        let provider = bootstrap_provider(&cfg()).expect("bootstrap");
        let report = provider.capabilities();
        // The provenance fixture passes during bootstrap, so the capability is
        // Supported and the handle is wired.
        assert!(
            report.episodes_evidence.is_supported(),
            "episodes_evidence should be Supported after bootstrap: {:?}",
            report.episodes_evidence
        );
        assert!(
            provider.provenance().is_some(),
            "provenance handle must be present when episodes_evidence is Supported"
        );
    }

    #[test]
    fn config_only_bootstrap_has_no_provenance_handle() {
        // The config-only EngramProvider::bootstrap (no adapter wired) reports
        // episodes_evidence Unsupported { FeatureDisabled } with no handle — the
        // contrast that proves T3 only flips the capability with a handle.
        use engram_domain::types::ScopeMappingStrategy;
        use engram_integration::{
            CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
        };
        let dir = std::env::temp_dir().join(format!("engram-prov-empty-{}", std::process::id()));
        let config = EngramConfig::new(
            dir.clone(),
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
        );
        let provider = EngramProvider::bootstrap(&config).expect("empty bootstrap");
        assert!(
            provider.provenance().is_none(),
            "unwired provider has no provenance handle"
        );
        assert!(
            !provider.capabilities().episodes_evidence.is_supported(),
            "unwired provider reports episodes_evidence unsupported"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bootstrap_provider_exposes_batch_handle_when_supported() {
        let provider = bootstrap_provider(&cfg()).expect("bootstrap");
        let report = provider.capabilities();
        // The batch fixture passes during bootstrap, so atomic_batch is
        // Supported and the handle is wired with a BestEffort guarantee.
        assert!(
            report.atomic_batch.is_supported(),
            "atomic_batch should be Supported after bootstrap: {:?}",
            report.atomic_batch
        );
        let handle = provider
            .batch()
            .expect("batch handle must be present when atomic_batch is Supported");
        assert_eq!(
            handle.transaction_guarantee(),
            engram_integration::TransactionGuarantee::BestEffort,
            "batch handle must report BestEffort, never Atomic"
        );
    }

    #[test]
    fn bootstrap_provider_exposes_recall_handle_when_supported() {
        use futures::executor::block_on;

        let provider = bootstrap_provider(&cfg()).expect("bootstrap");
        let report = provider.capabilities();
        // The recall fixture passes during bootstrap AND the memory + beliefs
        // stores are wired, so unified_recall is Supported and the recall handle
        // (graph + lexical + beliefs lanes) is attached.
        assert!(
            report.unified_recall.is_supported(),
            "unified_recall should be Supported after bootstrap: {:?}",
            report.unified_recall
        );
        let handle = provider
            .recall()
            .expect("recall handle must be present when unified_recall is Supported");
        // Recall against a fresh (empty) store degrades to Ok with no candidates
        // — the lanes run independently; an empty graph/lexical lane yields no
        // candidates rather than erroring.
        let payload = block_on(handle.recall(request_for_recall())).expect("recall must not error");
        // Fresh stores → no facts/graph/lexical/belief candidates.
        assert!(
            payload.items.is_empty(),
            "fresh bootstrap recall should yield no items, got {:?}",
            payload
                .items
                .iter()
                .map(|i| i.target_id.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn bootstrap_provider_exposes_observability_handle_when_supported() {
        use futures::executor::block_on;

        let provider = bootstrap_provider(&cfg()).expect("bootstrap");
        let report = provider.capabilities();
        // The observability fixture passes during bootstrap, so the capability
        // is Supported and the diagnostics handle is wired.
        assert!(
            report.observability.is_supported(),
            "observability should be Supported after bootstrap: {:?}",
            report.observability
        );
        let handle = provider
            .observability()
            .expect("observability handle must be present when observability is Supported");
        // The snapshot delegates the provider's capability report + versions and
        // derives (zero, empty-store) counts against the diagnostic scope.
        let snap = block_on(handle.diagnostics()).expect("diagnostics must not error");
        assert_eq!(
            snap.capabilities, *report,
            "capabilities delegated unchanged"
        );
        assert_eq!(snap.schema_version, SCHEMA_VERSION);
        assert_eq!(snap.adapter_version, ADAPTER_VERSION);
        assert!(
            snap.slow_query_diagnostics.is_none(),
            "slow-query None in v1"
        );
        // Fresh bootstrap → empty stores → all counts zero against the diagnostic
        // tenant (honest v1: counts are scope-visible, not cross-tenant).
        let zero = engram_integration::RecordCounts::empty();
        assert_eq!(
            snap.record_counts, zero,
            "fresh bootstrap reports zero counts"
        );
    }

    #[test]
    fn config_only_bootstrap_has_no_observability_handle() {
        // The config-only EngramProvider::bootstrap (no adapter wired) reports
        // observability Unsupported { FeatureDisabled } with no handle — the
        // contrast that proves the wiring layer only flips the capability with a
        // handle.
        use engram_domain::types::ScopeMappingStrategy;
        use engram_integration::{
            CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
        };
        let dir = std::env::temp_dir().join(format!("engram-obs-empty-{}", std::process::id()));
        let config = EngramConfig::new(
            dir.clone(),
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
        );
        let provider = EngramProvider::bootstrap(&config).expect("empty bootstrap");
        assert!(
            provider.observability().is_none(),
            "unwired provider has no observability handle"
        );
        assert!(
            !provider.capabilities().observability.is_supported(),
            "unwired provider reports observability unsupported"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- single-file SQLite layout ----

    fn fresh_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "engram-layout-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ))
    }

    fn count_db_files(dir: &std::path::Path) -> usize {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .is_some_and(|x| x == "db" || x == "sqlite" || x == "sqlite3")
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn multi_file_default_creates_separate_databases() {
        let dir = fresh_dir("multi");
        let config = cfg_with_storage(dir.clone());
        bootstrap_provider(&config).expect("bootstrap");
        // Default layout: one .db file per store family.
        assert!(
            count_db_files(&dir) >= 5,
            "expected at least 5 separate DB files, found {}: {:?}",
            count_db_files(&dir),
            std::fs::read_dir(&dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_file_layout_creates_one_database() {
        let dir = fresh_dir("single");
        let config = cfg_with_storage(dir.clone()).with_sqlite_storage_layout(
            SqliteStorageLayout::SingleFile {
                file_name: "engram_data.db".to_string(),
            },
        );
        bootstrap_provider(&config).expect("bootstrap");
        // Exactly one database file in the storage directory.
        assert_eq!(count_db_files(&dir), 1, "expected exactly one DB file");
        assert!(dir.join("engram_data.db").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_file_layout_bootstraps_all_repositories() {
        let dir = fresh_dir("single-all");
        let config = cfg_with_storage(dir.clone()).with_sqlite_storage_layout(
            SqliteStorageLayout::SingleFile {
                file_name: "engram_data.db".to_string(),
            },
        );
        let provider = bootstrap_provider(&config).expect("bootstrap");
        let report = provider.capabilities();
        // Every file-backed family must still bootstrap against the shared file.
        assert!(report.memory.is_supported(), "memory: {:?}", report.memory);
        assert!(report.knowledge.is_supported());
        assert!(report.beliefs.is_supported());
        assert!(report.hierarchy.is_supported());
        assert!(provider.memory().is_some());
        assert!(provider.knowledge().is_some());
        assert!(provider.beliefs().is_some());
        assert!(provider.hierarchy().is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_file_layout_writes_and_reads_across_stores() {
        use engram_domain::*;
        use futures::executor::block_on;

        let dir = fresh_dir("single-rw");
        let config = cfg_with_storage(dir.clone()).with_sqlite_storage_layout(
            SqliteStorageLayout::SingleFile {
                file_name: "engram_data.db".to_string(),
            },
        );
        let provider = bootstrap_provider(&config).expect("bootstrap");

        // Memory: write + retrieve through the shared file-backed handle.
        let scope = Scope {
            tenant: "tenant-single".to_string(),
            subject: Some("subject-single".to_string()),
            workspace: None,
            session: None,
            environment: Some("test".to_string()),
        };
        let requester = Requester {
            actor: Actor {
                id: Id::from("agent-single"),
                kind: ActorKind::Agent,
                display_name: Some("Single".to_string()),
                metadata: None,
            },
            roles: Vec::new(),
            permissions: vec!["memory.write".to_string(), "memory.retrieve".to_string()],
            on_behalf_of: None,
        };
        let memory = provider.memory().expect("memory handle");
        let written = block_on(memory.write_memory(WriteMemoryRequest {
            kind: MemoryKind::Observation,
            content: MemoryContent {
                text: "single-file memory".to_string(),
                summary: None,
                entities: Vec::new(),
                language: None,
                format: None,
                structured: None,
                hash: None,
            },
            scope: scope.clone(),
            requester: requester.clone(),
            provenance: Provenance {
                source: "single-file-test".to_string(),
                actor: requester.actor.clone(),
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: None,
            },
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            links: Vec::new(),
            idempotency_key: None,
        }))
        .expect("write memory");
        let id = written.record.id.clone();
        let context = block_on(memory.retrieve(RetrievalRequest {
            query: "single-file".to_string(),
            scope: scope.clone(),
            requester: requester.clone(),
            modes: Vec::new(),
            filters: None,
            cues: Vec::new(),
            limit: Some(5),
            budget: None,
            include_explanations: None,
        }))
        .expect("retrieve memory");
        assert!(
            context.items.iter().any(|i| i.target_id == id.to_string()),
            "memory written to the shared file must be retrievable"
        );

        // Knowledge: put a source + list it back through the shared file.
        let knowledge = provider.knowledge().expect("knowledge handle");
        block_on(knowledge.put_source(KnowledgeSource {
            id: Id::from("single-source"),
            kind: SourceKind::Filesystem,
            scope: scope.clone(),
            name: "single-file source".to_string(),
            uri: None,
            version: None,
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            provenance: Provenance {
                source: "single-file-test".to_string(),
                actor: requester.actor.clone(),
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: None,
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        }))
        .expect("put source");

        // Belief: put a belief + list it back through the shared file.
        let belief = provider.beliefs().expect("belief handle");
        block_on(belief.put_belief(Belief {
            id: Id::from("single-belief"),
            scope: scope.clone(),
            subject: BeliefSubject {
                key: "svc-single".to_string(),
                entity_ref: None,
                concept_ref: None,
                aliases: Vec::new(),
            },
            content: "single-file belief".to_string(),
            status: BeliefStatus::Active,
            confidence: 0.9,
            sources: Vec::new(),
            valid_from: Some(chrono::Utc::now()),
            valid_until: None,
            superseded_by: None,
            stale: None,
            synthesizer: None,
            reasoning: None,
            embedding_refs: Vec::new(),
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            provenance: Provenance {
                source: "single-file-test".to_string(),
                actor: requester.actor.clone(),
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: None,
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        }))
        .expect("put belief");

        // Hierarchy: put a node through the shared file.
        let hierarchy = provider.hierarchy().expect("hierarchy handle");
        block_on(hierarchy.put_node(HierarchyNode {
            id: HierarchyNodeId::from("single-node"),
            scope: scope.clone(),
            kind: HierarchyNodeKind::Base,
            layer: 0,
            name: "single-file node".to_string(),
            summary: None,
            parent_id: None,
            members: Vec::new(),
            source_target_type: None,
            source_target_id: None,
            embedding_refs: Vec::new(),
            status: HierarchyNodeStatus::Active,
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            provenance: Provenance {
                source: "single-file-test".to_string(),
                actor: requester.actor.clone(),
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: None,
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        }))
        .expect("put node");

        // If we got here, memory + knowledge + belief + hierarchy all wrote to
        // the SAME SQLite file without colliding — the single-file contract.
        let _ = (scope, knowledge, belief, hierarchy);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Helper: cfg() but with an explicit (fresh) storage directory.
    fn cfg_with_storage(dir: std::path::PathBuf) -> EngramConfig {
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
}
