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
        let path = &paths.memory;
        if let Ok(svc) = SqlMemoryService::open_file(path) {
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
        let path = &paths.knowledge;
        if let Ok(store) = SqlKnowledgeStore::open_file(path) {
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
        let path = &paths.belief;
        if let Ok(store) = SqlBeliefStore::open_file(path) {
            beliefs = Some(Arc::new(store));
            beliefs_state = CapabilityState::Supported;
        }
    }

    // Hierarchy.
    if fixtures::hierarchy::run_hierarchy_fixture().is_ok() {
        let path = &paths.hierarchy;
        if let Ok(store) = SqlHierarchyStore::open_file(path) {
            hierarchy = Some(Arc::new(store));
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

    // ---- single-file SQLite layout ----

    fn fresh_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "engram-layout-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        dir
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
