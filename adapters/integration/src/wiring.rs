//! Provider wiring entry point for the conformance crate.
//!
//! Historically this module owned the SQLite adapter construction + fixture
//! gating. That wiring has moved into `core/integration` behind the `sqlite`
//! feature (`engram_integration::sqlite::bootstrap_sqlite`), reached through
//! [`EngramProvider::open`](engram_integration::EngramProvider::open). This
//! module now keeps the public `bootstrap_provider` symbol as a thin delegate so
//! existing callers (`examples/rust-integration`, `bindings/node`, and the
//! conformance test suite) keep compiling against the same API while the
//! sole-runtime-dependency contract (RFC: integration-host-facade-v2) makes
//! `EngramProvider::open` the canonical host entry point.
//!
//! ADR-0022 boundary is preserved: the port impls live in the core crate's
//! `sqlite` module (engine-specific, exempt from the neutrality gate); the
//! engine-neutral port traits stay in the core crate's public surface.

use engram_integration::{EngramConfig, EngramProvider};
use engram_runtime::CoreResult;

/// Bootstraps a fully-wired provider from configuration.
///
/// Thin delegate to [`EngramProvider::open`]. Prefer calling
/// `EngramProvider::open` directly in new code; this function remains for
/// source compatibility with existing hosts that call
/// `engram_conformance::bootstrap_provider`.
///
/// # Errors
///
/// Returns `CoreError::InvalidRequest` if configuration validation fails, or
/// `CoreError::CapabilityUnsupported` if the `sqlite` feature is not enabled on
/// `engram-integration`.
pub fn bootstrap_provider(config: &EngramConfig) -> CoreResult<EngramProvider> {
    EngramProvider::open(config)
}

/// Storage schema version reported by provider diagnostics (test assertions).
#[cfg(test)]
const SCHEMA_VERSION: &str = "2026.01";

/// Adapter version reported by provider diagnostics (test assertions).
#[cfg(test)]
const ADAPTER_VERSION: &str = "0.1.0";

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::types::ScopeMappingStrategy;
    use engram_integration::{
        CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
        SqliteStorageLayout, TransactionGuarantee,
    };

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
        assert!(provider.retrieval().is_none());
        assert_eq!(provider.schema_version(), SCHEMA_VERSION);
        assert!(
            report.episodes_evidence.is_supported(),
            "episodes_evidence should be Supported: {:?}",
            report.episodes_evidence
        );
        assert!(provider.provenance().is_some());
        assert!(
            report.atomic_batch.is_supported(),
            "atomic_batch should be Supported: {:?}",
            report.atomic_batch
        );
        assert!(provider.batch().is_some());
        assert_eq!(
            provider
                .batch()
                .expect("batch handle")
                .transaction_guarantee(),
            TransactionGuarantee::BestEffort,
            "batch handle must report BestEffort"
        );
        assert!(
            report.unified_recall.is_supported(),
            "unified_recall should be Supported after bootstrap: {:?}",
            report.unified_recall
        );
        assert!(provider.recall().is_some());
        assert!(
            report.export_import.is_supported(),
            "export_import should be Supported: {:?}",
            report.export_import
        );
        assert!(provider.export_import().is_some());
        assert!(
            report.observability.is_supported(),
            "observability should be Supported: {:?}",
            report.observability
        );
        assert!(provider.observability().is_some());
    }

    #[test]
    fn supported_family_always_carries_a_handle() {
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
        assert!(!report.retrieval.is_supported());
        assert!(provider.retrieval().is_none());
        if report.episodes_evidence.is_supported() {
            assert!(provider.provenance().is_some());
        }
        if report.atomic_batch.is_supported() {
            assert!(provider.batch().is_some());
        }
        if report.unified_recall.is_supported() {
            assert!(provider.recall().is_some());
        }
        if report.export_import.is_supported() {
            assert!(provider.export_import().is_some());
        }
        if report.observability.is_supported() {
            assert!(provider.observability().is_some());
        }
        let _ = report; // silence unused on partial-failure builds
    }

    #[test]
    fn bootstrap_provider_exposes_recall_handle_when_supported() {
        use futures::executor::block_on;

        let provider = bootstrap_provider(&cfg()).expect("bootstrap");
        let report = provider.capabilities();
        assert!(report.unified_recall.is_supported());
        let handle = provider
            .recall()
            .expect("recall handle must be present when unified_recall is Supported");
        let payload = block_on(handle.recall(request_for_recall())).expect("recall must not error");
        assert!(payload.items.is_empty());
    }

    #[test]
    fn bootstrap_provider_exposes_observability_handle_when_supported() {
        use futures::executor::block_on;

        let provider = bootstrap_provider(&cfg()).expect("bootstrap");
        let report = provider.capabilities();
        assert!(report.observability.is_supported());
        let handle = provider
            .observability()
            .expect("observability handle must be present when observability is Supported");
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
        let zero = engram_integration::RecordCounts::empty();
        assert_eq!(
            snap.record_counts, zero,
            "fresh bootstrap reports zero counts"
        );
    }

    #[test]
    fn config_only_open_has_no_handles() {
        // The config-only EngramProvider::bootstrap (no adapter wired) reports
        // every handle absent — the contrast that proves open() only flips a
        // capability with a handle.
        use engram_domain::types::ScopeMappingStrategy;
        let dir = std::env::temp_dir().join(format!("engram-open-empty-{}", std::process::id()));
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
        assert!(provider.provenance().is_none());
        assert!(provider.observability().is_none());
        assert!(
            !provider.capabilities().episodes_evidence.is_supported(),
            "unwired provider reports episodes_evidence unsupported"
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
        assert!(
            count_db_files(&dir) >= 5,
            "expected at least 5 separate DB files, found {}",
            count_db_files(&dir)
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
