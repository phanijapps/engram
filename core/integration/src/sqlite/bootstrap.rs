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
use engram_domain::{CapabilityReason, CapabilityState, RerankStrategy};
use engram_hierarchy::HierarchyRepository;
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
};
use engram_memory::MemoryService;
use engram_procedures::ProcedureRepository;
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::SqlBeliefStore;
use engram_store_sqlite::SqlHierarchyStore;
use engram_store_sqlite::SqlIdentityStore;
use engram_store_sqlite::SqlKnowledgeStore;
use engram_store_sqlite::SqlMemoryService;
use engram_store_sqlite::SqlProcedureStore;

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
    procedures: PathBuf,
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
                    procedures: storage.join("procedures.db"),
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
                    procedures: shared.clone(),
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
    let mut procedures_state = failed();
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
    let mut identity_state = failed();
    // export_import is a shipped capability (S5).
    let mut export_import_state = failed();

    let mut memory: Option<Arc<dyn MemoryService>> = None;
    let mut knowledge: Option<Arc<dyn KnowledgeRepository>> = None;
    let mut knowledge_query: Option<Arc<dyn crate::KnowledgeQuery>> = None;
    let mut community_query: Option<Arc<dyn crate::CommunityQuery>> = None;
    let mut lexical_feed: Option<Arc<dyn crate::LexicalFeed>> = None;
    #[allow(unused_mut)]
    let mut embedding_provider: Option<Arc<dyn crate::EmbeddingProvider>> = None;
    let mut graph: Option<Arc<dyn KnowledgeGraphRepository>> = None;
    let mut ontology: Option<Arc<dyn OntologyRepository>> = None;
    let mut taxonomy: Option<Arc<dyn TaxonomyRepository>> = None;
    let mut beliefs: Option<Arc<dyn BeliefRepository>> = None;
    let mut procedures: Option<Arc<dyn ProcedureRepository>> = None;
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
    let mut identity: Option<Arc<dyn engram_knowledge::EntityIdentityRepository>> = None;
    // Shared graph snapshot cache: one Arc shared across the three graph lanes
    // (graph / associative / community-summary) so a cache miss in one lane
    // benefits the others on the next query — they all need the same scope's
    // entities + relationships. Eliminates the per-query store reload
    // (~300k JSON deserializations across the lanes) on every query after the
    // first. `scan_repo` invalidates it after a write so stale entries never
    // serve wrong results.
    let graph_cache: Arc<dyn engram_retrieval::GraphCache> =
        Arc::new(engram_retrieval::InMemoryGraphCache::new());
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
            knowledge_query = Some(store.clone());
            community_query = Some(store.clone());
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
            // Identity: construct SqlIdentityStore from the shared connection.
            identity = Some(Arc::new(SqlIdentityStore::new(store.shared_connection())));
            identity_state = CapabilityState::Supported;
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

    // Procedures (RFC-0016 Layer 6).
    if conformance::procedures_ok() {
        if let Ok(store) = SqlProcedureStore::open_file(&paths.procedures) {
            let store: Arc<SqlProcedureStore> = Arc::new(store);
            procedures = Some(store);
            procedures_state = CapabilityState::Supported;
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
        // Graph lane: SqlKnowledgeStore implements GraphCandidateSource. The
        // lane reads ENTITIES from the shared cache on a hit; chunks are always
        // loaded from the store (not part of the snapshot).
        retrieval_lanes.push(Arc::new(
            engram_store_sqlite::GraphRetrievalIndex::with_cache(
                knowledge_handle.clone(),
                20,
                graph_cache.clone(),
            ),
        ));
        // Associative-graph lane: PPR-ranked entities over the knowledge graph
        // (HippoRAG-style), fused alongside the other unified-recall lanes.
        // Populates the shared cache (entities + relationships) on a miss.
        retrieval_lanes.push(recall_lanes::associative_recall_lane(
            knowledge_handle.clone(),
            Some(graph_cache.clone()),
        ));
        // Community-summary lane (GraphRAG): community detection + summary ranking.
        // Populates the shared cache (entities + relationships) on a miss.
        retrieval_lanes.push(recall_lanes::community_summary_recall_lane(
            knowledge_handle.clone(),
            Some(graph_cache.clone()),
        ));
        // Lexical lane: a **file-backed** Tantivy index shared with the lexical
        // feed, persisted at `<storage_path>/lexical` so it survives process
        // restarts. `scan_repo` feeds code-symbol names via the `LexicalFeed`
        // handle so keyword `search`/`recall` return them; with the index on
        // disk, those writes are visible to a FRESH process that has not run
        // `scan_repo` (the cross-process search guarantee). `LexicalIndex::open`
        // creates the directory if missing, loads an existing same-schema index,
        // or falls back to an ephemeral in-RAM index (with a warning) on any
        // open/corruption failure — it never aborts bootstrap. The lane's target
        // resolver is entity-aware (it resolves entity-id BM25 hits to their
        // code symbol), so multi-term symbol queries return ranked hits through
        // unified recall.
        let lexical_dir = storage.join("lexical");
        if let Ok(lexical_index) = engram_store_lexical::LexicalIndex::open(&lexical_dir) {
            let lexical_index = Arc::new(lexical_index);
            retrieval_lanes.push(recall_lanes::lexical_recall_lane(
                knowledge_handle.clone(),
                lexical_index.clone(),
            ));
            lexical_feed = Some(Arc::new(crate::sqlite::lexical_feed::SqlLexicalFeed::new(
                lexical_index,
            )));
        }
    }
    // Temporal lane (recency-weighted memories): available whenever the memory
    // store is wired, independent of the knowledge store. Gives recall a recency
    // signal alongside the relevance lanes (graph/vector/lexical).
    if let Some(memory_handle) = &memory_store {
        retrieval_lanes.push(Arc::new(engram_store_sqlite::TemporalRetrievalIndex::new(
            memory_handle.clone(),
        )));
    }
    eprintln!(
        "engram-mcp: vector bootstrap checkpoint — knowledge_store={}, vectors_path={:?}",
        knowledge_store.is_some(),
        paths.vectors
    );
    // Vector lane (fastembed-gated): construct the vector index + FastEmbed
    // query provider + knowledge-store resolver. Skipped entirely when the
    // feature is off (default build) or when construction fails — recall then
    // runs degraded for vector (fewer candidates), never errors.
    //
    // RFC-0019 D3 (reversed): additionally gated on `config.enable_vector`
    // (default `true`). When an operator passes `enable_vector = false` (MCP
    // `--no-vector`), the whole block is skipped — no `SqliteVectorIndex`, no
    // `FastEmbedBgeSmallQueryProvider` (so no model download/load), no vector
    // recall lane — even though fastembed is compiled in. The capability is
    // left `Unsupported` with `vectors = None`.
    #[cfg(feature = "fastembed")]
    {
        if !config.enable_vector {
            eprintln!("engram-mcp: vector lane disabled by config (enable_vector=false)");
        } else if let (Some(knowledge_handle), Some(path_str)) =
            (&knowledge_store, paths.vectors.to_str())
        {
            let dims = config.embedding_provider.dimensions;
            let space = engram_domain::EmbeddingSpace::new(
                &config.embedding_provider.provider_type,
                &config.embedding_provider.model,
                dims,
                &config.embedding_provider.prompt_profile,
                config.embedding_provider.normalization.clone(),
            );
            if let Ok(vector_index) =
                engram_store_sqlite::SqliteVectorIndex::open_with_embedding_space(
                    path_str,
                    space.clone(),
                )
            {
                match engram_store_sqlite::FastEmbedBgeSmallQueryProvider::new() {
                    Ok(query_provider) => {
                        let provider_arc: Arc<engram_store_sqlite::FastEmbedBgeSmallQueryProvider> =
                            Arc::new(query_provider);
                        let resolver =
                            recall_lanes::KnowledgeVectorResolver::new(knowledge_handle.clone());
                        let vec_index = Arc::new(vector_index);
                        retrieval_lanes.push(Arc::new(
                            engram_store_sqlite::VectorRetrievalIndex::new(
                                (*vec_index).clone(),
                                provider_arc.clone(),
                                Arc::new(resolver),
                            ),
                        ));
                        vectors = Some(vec_index as Arc<dyn engram_retrieval::VectorIndex>);
                        vectors_state = CapabilityState::Supported;
                        embedding_provider = Some(Arc::new(
                            super::fastembed_embedding::FastEmbedEmbeddingProvider::new(
                                provider_arc,
                                space,
                            ),
                        ));
                    }
                    Err(e) => {
                        eprintln!("engram-mcp: FastEmbed init failed — vector lane disabled: {e}");
                    }
                }
            } else {
                eprintln!("engram-mcp: SqliteVectorIndex open failed — vector lane disabled");
            }
        }
    }
    if let (Some(memory_handle), Some(belief_handle)) = (&memory_store, &belief_store)
        && conformance::recall_ok()
    {
        // RFC-0019: build the weighted-RRF fusion config from the operator-facing
        // `recall_fusion` config (`[recall_fusion]` profile section or
        // `.engram/recall.json`); fall back to equal-weight default when absent.
        // Validation already ran at load, so `to_reciprocal_config()` only
        // errors if the config was constructed directly with bad weights —
        // surfaced as a typed boot error rather than a silent degrade.
        let fusion = match config.recall_fusion.as_ref() {
            Some(cfg) => cfg
                .to_reciprocal_config()
                .map_err(|e| CoreError::InvalidRequest {
                    reason: format!("invalid recall_fusion config: {e}"),
                })?,
            None => engram_retrieval::ReciprocalFusionConfig::default(),
        };
        // RFC-0019 T3/T4: select the reranker from `recall_fusion.rerank`.
        // `None`/`Mmr`/`CrossEncoder` dispatched by strategy. MMR needs a wired
        // `EmbeddingProvider` (fastembed on); it degrades to relevance-only
        // with a warning when absent, never a panic. Cross-encoder is behind
        // its feature gate; it warns + falls back when no scorer is wired.
        let reranker = select_reranker(
            config
                .recall_fusion
                .as_ref()
                .and_then(|c| c.rerank.as_ref()),
            embedding_provider.as_ref(),
        );
        let unified = SqlUnifiedRecall::with_reranker(
            memory_handle.clone(),
            retrieval_lanes,
            belief_handle.clone(),
            fusion,
            reranker,
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
    //
    // Also gated on `config.enable_vector` (RFC-0019 D3 reversed): when an
    // operator disables vector at runtime, neither this block nor the
    // fastembed lane above constructs a vector index, so `vectors` stays `None`
    // and `vectors_state` stays `Unsupported`.
    if config.enable_vector && conformance::vector_ok() {
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

    // Contradiction CRUD (list/put) rides the wired belief repository. Rule-based
    // detect is NOT exposed on the provider (it stays on the standalone belief
    // engine); the LLM reflect/contradict ops + LLM detect are TS-only, env-gated
    // (PI_PROVIDER / provider key). Maintenance = consolidate (here) + the TS LLM
    // layer. Both track their store wiring; the LLM half's runtime availability
    // additionally depends on env config (a missing key fails at call time, not here).
    let contradiction_state = beliefs_state.clone();
    let maintenance_state = consolidation_state.clone();

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
        .contradiction(contradiction_state)
        .maintenance(maintenance_state)
        .identity(identity_state)
        .procedures(procedures_state)
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
    if let Some(h) = knowledge_query {
        builder = builder.knowledge_query(h);
    }
    if let Some(h) = community_query {
        builder = builder.community_query(h);
    }
    if let Some(h) = lexical_feed {
        builder = builder.lexical_feed(h);
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

    if let Some(h) = procedures {
        builder = builder.procedures(h);
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
    if let Some(h) = embedding_provider {
        builder = builder.embedding_provider(h);
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
    if let Some(h) = identity {
        builder = builder.identity(h);
    }
    // Attach the shared graph cache so `scan_repo` (and any host) can reach it
    // through the facade to invalidate after a graph-mutating write. The cache
    // is always present under the SQLite backend (no fallback when a lane is
    // absent — it is a benign empty cache in that case).
    builder = builder.graph_cache(graph_cache);
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

// ---- RFC-0019 T3/T4: reranker dispatch -----------------------------------
// Selects the reranker for unified recall from the operator-facing
// `recall_fusion.rerank` config. ADR-0022: this is the engine-specific wiring
// site (names adapter crates + the integration `EmbeddingProvider`); the
// reranker port + dispatch contract stay engine-neutral.

/// Selects the reranker by `rerank.strategy`. Returns `None` (no rerank) when
/// no rerank config is set, strategy is `None`, or the selected reranker's
/// prerequisites are unmet (MMR without an embedder; cross-encoder without its
/// feature/scorer). Unmet prerequisites degrade with a warning, never a panic.
fn select_reranker(
    rerank: Option<&engram_retrieval::RerankConfig>,
    embedding_provider: Option<&Arc<dyn crate::EmbeddingProvider>>,
) -> Option<Arc<dyn engram_retrieval::RetrievalReranker>> {
    let Some(cfg) = rerank else {
        return None;
    };
    match cfg.strategy {
        RerankStrategy::None => None,
        RerankStrategy::Mmr => select_mmr(embedding_provider, cfg.lambda),
        RerankStrategy::CrossEncoder => select_cross_encoder(),
        // `LlmJudge` / `PolicyPriority` are not dispatched by engram today.
        _ => None,
    }
}

/// MMR needs an `EmbeddingProvider` to embed candidate texts (RFC-0019 D4 — the
/// `RetrievalReranker` port exposes no embeddings). The embedder is wired only
/// under the `fastembed` feature; without it (or when construction failed), MMR
/// degrades to relevance-only ordering with a warning.
fn select_mmr(
    embedding_provider: Option<&Arc<dyn crate::EmbeddingProvider>>,
    lambda: f32,
) -> Option<Arc<dyn engram_retrieval::RetrievalReranker>> {
    #[cfg(feature = "fastembed")]
    {
        if let Some(emb) = embedding_provider {
            // Bridge the integration `EmbeddingProvider` to the MMR adapter's
            // local `MmrEmbedder` trait (the adapter cannot depend on the
            // facade without a package cycle — see `engram-rerank-mmr` docs).
            let embedder: Arc<dyn engram_rerank_mmr::MmrEmbedder> =
                Arc::new(FastEmbedMmrEmbedder { inner: emb.clone() });
            return Some(Arc::new(engram_rerank_mmr::MmrReranker::new(
                Some(embedder),
                lambda,
            )));
        }
        eprintln!(
            "engram-mcp: MMR reranker selected but no embedding provider is wired \
             (enable the `fastembed` feature); falling back to no rerank"
        );
        None
    }
    #[cfg(not(feature = "fastembed"))]
    {
        let _ = embedding_provider;
        let _ = lambda;
        eprintln!(
            "engram-mcp: MMR reranker selected but the `fastembed` feature is disabled \
             (no embedding provider); falling back to no rerank"
        );
        None
    }
}

/// Cross-encoder is behind the `cross-encoder-rerank` feature (RFC-0019 T4 /
/// D2.1c). The adapter is constructible when a deployer wires a `RerankScorer`;
/// no model ships in-tree today (backlog `cross-encoder-rerank`), so selection
/// warns + falls back to no rerank rather than inventing a scorer.
fn select_cross_encoder() -> Option<Arc<dyn engram_retrieval::RetrievalReranker>> {
    #[cfg(feature = "cross-encoder-rerank")]
    {
        eprintln!(
            "engram-mcp: cross-encoder reranker selected but no RerankScorer model is \
             wired; falling back to no rerank (see backlog `cross-encoder-rerank`)"
        );
        None
    }
    #[cfg(not(feature = "cross-encoder-rerank"))]
    {
        eprintln!(
            "engram-mcp: cross-encoder reranker selected but the `cross-encoder-rerank` \
             feature is disabled; falling back to no rerank"
        );
        None
    }
}

/// Bridge: adapts the integration [`crate::EmbeddingProvider`] to the MMR
/// adapter's local `MmrEmbedder` trait. Only the fastembed build wires an
/// embedding provider, so the bridge lives behind that feature.
#[cfg(feature = "fastembed")]
struct FastEmbedMmrEmbedder {
    inner: Arc<dyn crate::EmbeddingProvider>,
}

#[cfg(feature = "fastembed")]
impl engram_rerank_mmr::MmrEmbedder for FastEmbedMmrEmbedder {
    fn embed(&self, text: &str) -> CoreResult<Vec<f32>> {
        // Candidate texts are passages (not queries), so embed as a passage.
        self.inner.embed_passage(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, MigrationMode};
    use engram_domain::ScopeMappingStrategy;

    fn vector_config(dir: &std::path::Path, enable_vector: bool) -> EngramConfig {
        let mut cfg = EngramConfig::new(
            dir.join("engram"),
            dir,
            ScopeMappingStrategy::Strict,
            EmbeddingProviderConfig {
                provider_type: "fastembed".to_string(),
                model: "BAAI/bge-small-en-v1.5".to_string(),
                dimensions: 384,
                prompt_profile: "query".to_string(),
                normalization: None,
            },
            MigrationMode::DryRun,
            CapabilityPolicy::FailClosed,
        );
        cfg = cfg.with_enable_vector(enable_vector);
        cfg
    }

    /// RFC-0019 D3 (reversed): when `enable_vector = false`, the vector lane is
    /// skipped entirely even though the `fastembed` feature is compiled in — no
    /// `SqliteVectorIndex`, no `FastEmbedBgeSmallQueryProvider` (so no model
    /// download/load), no vector recall lane. The capability must report
    /// `Unsupported` with no handle. Gated on `fastembed` because the runtime
    /// disable is specifically the "compiled-in but turned off" path; without
    /// the feature there is no model to skip.
    #[cfg(feature = "fastembed")]
    #[test]
    fn enable_vector_false_skips_vector_lane_under_fastembed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = vector_config(dir.path(), false);
        let provider = bootstrap_sqlite(&config).expect("bootstrap opens with vector disabled");

        // No vector handle attaches.
        assert!(
            provider.vectors().is_none(),
            "enable_vector=false must not wire a vector handle"
        );
        // The capability is not Supported.
        assert!(
            !provider.capabilities().vectors_supported(),
            "enable_vector=false must leave the vectors capability unsupported"
        );
    }

    /// The matching half: with `enable_vector = true` (the default) the vector
    /// lane is wired. Gated to the non-fastembed build so the assertion stays
    /// deterministic (under fastembed the first vector block attempts a model
    /// load; the SQLite-only second block wires `vectors` here without any
    /// model). The fastembed-gated disable test above plus this positive test
    /// together pin the gate from both sides.
    #[cfg(not(feature = "fastembed"))]
    #[test]
    fn enable_vector_true_wires_vector_lane() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = vector_config(dir.path(), true);
        let provider = bootstrap_sqlite(&config).expect("bootstrap opens with vector enabled");

        assert!(
            provider.vectors().is_some(),
            "enable_vector=true must wire a vector handle"
        );
        assert!(
            provider.capabilities().vectors_supported(),
            "enable_vector=true must mark the vectors capability supported"
        );
    }

    /// cc-pi-mono-maintenance T6: the SQLite bootstrap flips `contradiction` +
    /// `maintenance` to Supported (they ride the wired belief repository +
    /// consolidation path). Pins the flip so a future refactor that re-disables
    /// them ships red. Uses enable_vector=false to skip the model load.
    #[test]
    fn sqlite_bootstrap_flips_contradiction_and_maintenance_supported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = vector_config(dir.path(), false);
        let provider = bootstrap_sqlite(&config).expect("bootstrap opens");
        let report = provider.capabilities();
        assert!(
            report.contradiction_supported(),
            "SQLite bootstrap must mark contradiction Supported (belief repository wired)"
        );
        assert!(
            report.maintenance_supported(),
            "SQLite bootstrap must mark maintenance Supported (consolidation wired)"
        );
    }
}
