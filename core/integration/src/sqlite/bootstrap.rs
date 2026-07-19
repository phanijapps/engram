//! SQLite backend wiring: construct file-backed adapters from configuration,
//! gate each capability family on an inlined conformance check, and build a
//! fully-wired [`EngramProvider`] through [`EngramProviderBuilder`].
//!
//! This is the engine-specific counterpart to the engine-neutral provider
//! facade: adapter construction and capability detection live here (under the
//! `sqlite` feature), while the provider struct and trait handles live in the
//! port-only `provider.rs`. [`bootstrap_sqlite`] is the single entry point
//! reached by [`EngramProvider::open`](crate::EngramProvider::open) when the
//! `sqlite` feature is enabled.
//!
//! ADR-0022: this module names `Sql*` and holds the engine adapters by design;
//! it is intentionally exempt from the engine-neutrality gate.

use std::path::PathBuf;
use std::sync::Arc;

use engram_belief::BeliefRepository;
use engram_domain::{CapabilityReason, CapabilityState};
use engram_hierarchy::HierarchyRepository;
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
};
use engram_memory::MemoryService;
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::SqlBeliefStore;
use engram_store_sqlite::SqlHierarchyStore;
use engram_store_sqlite::SqlKnowledgeStore;
use engram_store_sqlite::SqlMemoryService;

use crate::{
    CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder, SqliteStorageLayout,
};

use super::conformance;
use super::recall_lanes;
use super::{
    SqlBatchIngest, SqlExportImport, SqlMigrationService, SqlObservability, SqlProvenanceQuery,
    SqlUnifiedRecall,
    consolidation_adapters::{
        ActiveMemorySourceAdapter, BeliefSinkAdapter, DecayMemorySourceAdapter,
        ExecutorConsolidationService,
    },
};
use engram_consolidation::{CompositeConsolidationExecutor, ConsolidationService};
use engram_decay::DecayExecutor;
use engram_reflection::{ReflectionExecutor, ReflectionSynthesizer};

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

/// Bootstraps a fully-wired provider from configuration against the SQLite
/// backend.
///
/// For each capability family this constructs the file-backed SQLite adapter at
/// `<storage_path>/<family>.db`, runs the corresponding inlined conformance
/// check against an in-memory store, and attaches the handle + marks the family
/// `Supported` only when the check passes. A family whose check fails is
/// reported `Unsupported` (`ConformanceFailed`) with no handle, so callers can
/// never reach a broken adapter through the facade.
///
/// This function is **synchronous** even though the underlying trait handles
/// are async: the SQLite adapters are sync rusqlite bodies wrapped in
/// async-by-convention trait methods, so `futures::executor::block_on` polls
/// each to completion in a single step without yielding. A host runtime is not
/// required on the open path.
///
/// # Errors
///
/// Returns `CoreError::InvalidRequest` if configuration validation fails, or
/// `CoreError::Adapter` if the storage directory cannot be created.
pub(crate) fn bootstrap_sqlite(config: &EngramConfig) -> CoreResult<EngramProvider> {
    config.validate().map_err(|e| CoreError::InvalidRequest {
        reason: format!("configuration validation failed: {e}"),
    })?;

    let storage = &config.storage_path;
    std::fs::create_dir_all(storage).map_err(|e| CoreError::Adapter {
        adapter: "engram-integration.sqlite".to_string(),
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
    let mut vectors_state = failed();
    // episodes_evidence is a shipped capability (S2): a check failure reports
    // ConformanceFailed, not FeatureDisabled. Flipped to Supported below only
    // when the provenance check passes and the handle attaches.
    let mut episodes_evidence_state = failed();
    // atomic_batch is a shipped capability (S3): same pattern.
    let mut atomic_batch_state = failed();
    // unified_recall is a shipped capability (S4).
    let mut unified_recall_state = failed();
    // consolidation is a shipped capability.
    let mut consolidation_state = failed();
    // export_import is a shipped capability (S5).
    let mut export_import_state = failed();

    let mut memory: Option<Arc<dyn MemoryService>> = None;
    let mut knowledge: Option<Arc<dyn KnowledgeRepository>> = None;
    let mut graph: Option<Arc<dyn KnowledgeGraphRepository>> = None;
    let mut ontology: Option<Arc<dyn OntologyRepository>> = None;
    let mut taxonomy: Option<Arc<dyn TaxonomyRepository>> = None;
    let mut beliefs: Option<Arc<dyn BeliefRepository>> = None;
    let mut hierarchy: Option<Arc<dyn HierarchyRepository>> = None;
    let retrieval: Option<Arc<dyn engram_retrieval::RetrievalIndex>> = None;
    let mut vectors: Option<Arc<dyn engram_retrieval::VectorIndex>> = None;
    let mut provenance: Option<Arc<dyn crate::ProvenanceQuery>> = None;
    let mut batch: Option<Arc<dyn crate::BatchIngest>> = None;
    let mut recall: Option<Arc<dyn crate::UnifiedRecall>> = None;
    let mut migration: Option<Arc<dyn crate::MigrationService>> = None;
    let mut export_import: Option<Arc<dyn crate::ExportImport>> = None;
    let mut observability: Option<Arc<dyn crate::Observability>> = None;
    let mut consolidation: Option<Arc<dyn ConsolidationService>> = None;
    // Concrete Sql* handles, kept alongside the trait handles so the batch /
    // export / observability adapters (which compose the concrete stores) can be
    // wired without a trait-to-concrete downcast. Populated only when the
    // corresponding family's handle attaches.
    let mut memory_store: Option<Arc<SqlMemoryService>> = None;
    let mut knowledge_store: Option<Arc<SqlKnowledgeStore>> = None;
    // SqlBeliefStore is kept concrete (alongside the trait handle) so the
    // observability / export adapters can call its `list_beliefs`.
    let mut belief_store: Option<Arc<SqlBeliefStore>> = None;
    // SqlHierarchyStore is kept concrete so the export adapter can call its
    // `list_nodes`.
    let mut hierarchy_store: Option<Arc<SqlHierarchyStore>> = None;

    // Memory: run the inlined conformance check, then attach a durable
    // file-backed handle.
    if conformance::memory_ok() {
        let path = &paths.memory;
        if let Ok(svc) = SqlMemoryService::open_file(path) {
            let svc: Arc<SqlMemoryService> = Arc::new(svc);
            memory_store = Some(svc.clone());
            memory = Some(svc);
            memory_state = CapabilityState::Supported;
        }
    }

    // Knowledge + graph + ontology + taxonomy share one SqlKnowledgeStore.
    let knowledge_ok = conformance::knowledge_ok();
    let graph_ok = conformance::graph_ok();
    let ontology_ok = conformance::ontology_ok();
    let taxonomy_ok = conformance::taxonomy_ok();
    let provenance_ok = conformance::provenance_ok();
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
            // the capability to Supported only when the check passes.
            if provenance_ok {
                provenance = Some(Arc::new(SqlProvenanceQuery::new(store.clone())));
                episodes_evidence_state = CapabilityState::Supported;
            }
        }
    }

    // atomic_batch (S3): a best-effort batch composes the memory + knowledge
    // stores. The handle is attached and the capability flipped to Supported
    // only when the check passes AND both file-backed stores are wired.
    if conformance::batch_ok()
        && let (Some(memory_handle), Some(knowledge_handle)) = (&memory_store, &knowledge_store)
    {
        batch = Some(Arc::new(SqlBatchIngest::new(
            memory_handle.clone(),
            knowledge_handle.clone(),
        )));
        atomic_batch_state = CapabilityState::Supported;
    }

    // Beliefs.
    if conformance::belief_ok() {
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
    // flip the capability to Supported when the conformance check passes AND the
    // memory + beliefs stores are wired.
    //
    // The v1 lanes compose as:
    //   - facts (memory) — passed to the SqlUnifiedRecall constructor below.
    //   - graph — GraphRetrievalIndex over SqlKnowledgeStore.
    //   - lexical — LexicalRetrievalIndex over an in-RAM LexicalIndex + a
    //     knowledge-store-backed target resolver (empty until a feed is added).
    //   - vector — feature-gated behind `fastembed` (off by default).
    //   - beliefs — passed to the SqlUnifiedRecall constructor below.
    let mut retrieval_lanes: Vec<Arc<dyn engram_retrieval::RetrievalIndex>> = Vec::new();
    if let Some(knowledge_handle) = &knowledge_store {
        // Graph lane: SqlKnowledgeStore implements GraphCandidateSource.
        retrieval_lanes.push(Arc::new(engram_store_sqlite::GraphRetrievalIndex::new(
            knowledge_handle.clone(),
        )));
        // Associative-graph lane: PPR-ranked entities over the knowledge graph
        // (HippoRAG-style), fused alongside the other unified-recall lanes.
        retrieval_lanes.push(recall_lanes::associative_recall_lane(
            knowledge_handle.clone(),
        ));
        // Community-summary lane (GraphRAG): community detection + summary ranking.
        retrieval_lanes.push(recall_lanes::community_summary_recall_lane(
            knowledge_handle.clone(),
        ));
        // Lexical lane: an in-RAM Tantivy index (no ingest feed wired here) +
        // a knowledge-store-backed target resolver.
        if let Ok(lexical_index) = engram_store_lexical::LexicalIndex::new() {
            let resolver = recall_lanes::KnowledgeLexicalResolver::new(knowledge_handle.clone());
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
            engram_store_sqlite::SqliteVectorIndex::open_with_embedding_space(path_str, space)
        {
            if let Ok(query_provider) = engram_store_sqlite::FastEmbedBgeSmallQueryProvider::new() {
                let resolver = recall_lanes::KnowledgeVectorResolver::new(knowledge_handle.clone());
                retrieval_lanes.push(Arc::new(engram_store_sqlite::VectorRetrievalIndex::new(
                    vector_index,
                    Arc::new(query_provider),
                    Arc::new(resolver),
                )));
            }
        }
    }
    if let (Some(memory_handle), Some(belief_handle)) = (&memory_store, &belief_store)
        && conformance::recall_ok()
    {
        let unified = SqlUnifiedRecall::new(
            memory_handle.clone(),
            retrieval_lanes,
            belief_handle.clone(),
        );
        recall = Some(Arc::new(unified));
        unified_recall_state = CapabilityState::Supported;
    }

    // Consolidation (reflection + decay via composite executor).
    if let (Some(mem), Some(bel)) = (&memory_store, &belief_store) {
        let sink = Arc::new(BeliefSinkAdapter(bel.clone()));
        let memory_source = Arc::new(ActiveMemorySourceAdapter(mem.clone()));
        let decay_source = Arc::new(DecayMemorySourceAdapter(mem.clone()));
        let now = chrono::Utc::now();
        let synthesizer = Arc::new(ReflectionSynthesizer::new(memory_source, now));
        let reflection_executor = Arc::new(ReflectionExecutor::new(synthesizer, sink));
        let decay_executor = Arc::new(DecayExecutor::new(decay_source));
        let composite = Arc::new(CompositeConsolidationExecutor::new(vec![
            reflection_executor,
            decay_executor,
        ]));
        consolidation = Some(Arc::new(ExecutorConsolidationService::new(composite)));
        consolidation_state = CapabilityState::Supported;
    }

    // Hierarchy.
    if conformance::hierarchy_ok() {
        let path = &paths.hierarchy;
        if let Ok(store) = SqlHierarchyStore::open_file(path) {
            let store: Arc<SqlHierarchyStore> = Arc::new(store);
            hierarchy_store = Some(store.clone());
            hierarchy = Some(store);
            hierarchy_state = CapabilityState::Supported;
        }
    }

    // Vectors: construct a file-backed SqliteVectorIndex configured with the
    // embedding space from configuration, then attach it. The check proves the
    // VectorIndex contract; the attached index is the usable instance.
    if conformance::vector_ok() {
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
                engram_store_sqlite::SqliteVectorIndex::open_with_embedding_space(path_str, space)
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
    // Retrieval: the trace contract is verified by the check, but no
    // RetrievalIndex adapter (context composer) is constructed here — that
    // lives in the orchestration layer. Report Unsupported honestly rather than
    // claiming Supported without a handle.
    let _ = conformance::retrieval_ok();
    let retrieval_state = CapabilityState::Unsupported {
        reason: CapabilityReason::UnsupportedStoreFamily,
    };

    // Migration: the fingerprint contract is verified by the check, and a real
    // SqlMigrationService handle is attached.
    let migration_state = if conformance::migration_ok() {
        let svc = SqlMigrationService::new(config.embedding_provider.dimensions);
        migration = Some(Arc::new(svc));
        CapabilityState::Supported
    } else {
        failed()
    };

    // export_import (S5): construct SqlExportImport from the wired file-backed
    // memory + knowledge stores. Gated on the check passing AND the migration
    // handle being wired (export + import are both needed for backend-to-backend
    // scope movement). Composes the concrete stores; belief + hierarchy attached
    // optionally so the export covers those families when wired.
    if conformance::export_import_ok()
        && migration.is_some()
        && let (Some(memory_handle), Some(knowledge_handle)) = (&memory_store, &knowledge_store)
    {
        let mut exporter = SqlExportImport::new(knowledge_handle.clone(), memory_handle.clone());
        if let Some(belief_handle) = belief_store.clone() {
            exporter = exporter.with_belief(belief_handle);
        }
        if let Some(hierarchy_handle) = hierarchy_store.clone() {
            exporter = exporter.with_hierarchy(hierarchy_handle);
        }
        export_import = Some(Arc::new(exporter));
        export_import_state = CapabilityState::Supported;
    }

    // observability (S6): the handle aggregates the provider's existing
    // diagnostics + derives record counts by listing the wired concrete
    // knowledge + belief stores.
    let observability_ok = conformance::observability_ok();
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
        .consolidation(consolidation_state)
        .build();

    // Construct the SqlObservability handle from the wired concrete stores +
    // the final capability report (delegated, not recomputed). Only attached
    // when the check passed; clones the report (Clone) so the provider keeps
    // the canonical copy.
    if observability_ok {
        observability = Some(Arc::new(SqlObservability::new(
            knowledge_store.clone(),
            belief_store.clone(),
            diagnostic_scope(),
            report.clone(),
            config.embedding_provider.clone(),
            SCHEMA_VERSION,
            ADAPTER_VERSION,
        )));
    }

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
    if let Some(h) = consolidation {
        builder = builder.consolidation(h);
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
