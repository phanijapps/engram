//! Open the [`EngramProvider`] that every tool routes through.
//!
//! This is the single place the server touches a backend: it builds an
//! [`EngramConfig`] (embedding-`none`, SQLite) and calls
//! [`EngramProvider::open`]. No tool opens a store directly.

use engram_integration::{EngramConfig, EngramProvider, SqliteStorageLayout};

use crate::config::{McpConfig, McpSqliteLayout};

/// Open the provider described by `config`. Errors are surfaced as a string
/// for the caller (`main`) to report.
pub fn open_provider(config: &McpConfig) -> Result<EngramProvider, String> {
    // `EngramProvider::open` validates that `storage_path` is inside
    // `trusted_root`, so default the trusted root to the storage path's parent
    // (mirroring `EngramConfig::from_profile_file`) instead of a hardcoded
    // temp dir — otherwise real `--storage` paths outside /tmp fail to boot.
    let trusted_root = config
        .storage_path
        .parent()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let layout = match config.sqlite_layout {
        McpSqliteLayout::Single => SqliteStorageLayout::SingleFile {
            file_name: config.db_file.clone(),
        },
        McpSqliteLayout::Multi => SqliteStorageLayout::MultiFileDirectory,
    };
    let engram_config = EngramConfig::new(
        config.storage_path.clone(),
        trusted_root,
        config.scope_strategy,
        config.embedding.clone(),
        config.migration_mode,
        config.capability_policy,
    )
    .with_sqlite_storage_layout(layout);
    EngramProvider::open(&engram_config).map_err(|e| format!("failed to open engram provider: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::ScopeMappingStrategy;
    use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, MigrationMode};

    fn test_config(dir: &std::path::Path) -> McpConfig {
        McpConfig {
            // `storage_path` is the directory the shared database file lives in.
            storage_path: dir.to_path_buf(),
            project: "test".to_string(),
            scope_strategy: ScopeMappingStrategy::Strict,
            embedding: EmbeddingProviderConfig {
                provider_type: "none".to_owned(),
                model: "none".to_owned(),
                dimensions: 384,
                prompt_profile: "query".to_owned(),
                normalization: None,
            },
            migration_mode: MigrationMode::DryRun,
            capability_policy: CapabilityPolicy::FailClosed,
            ontology_path: None,
            taxonomy_path: None,
            sqlite_layout: McpSqliteLayout::default(),
            db_file: "engram_data.db".to_string(),
            org: None,
            domain: None,
            subdomain: None,
        }
    }

    /// Goal-based: opening a provider under the SQLite feature wires the
    /// handles the Phase-1 tools depend on (spec AC: bootstrap returns a
    /// provider with these `Some`).
    #[test]
    fn open_provider_wires_core_handles() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = test_config(dir.path());
        let provider = open_provider(&config).expect("provider opens under sqlite");

        for (label, present) in [
            ("memory", provider.memory().is_some()),
            ("knowledge", provider.knowledge().is_some()),
            ("ontology", provider.ontology().is_some()),
            ("taxonomy", provider.taxonomy().is_some()),
            ("hierarchy", provider.hierarchy().is_some()),
            ("recall", provider.recall().is_some()),
            ("batch", provider.batch().is_some()),
            ("consolidation", provider.consolidation().is_some()),
        ] {
            assert!(present, "expected `{label}` handle to be wired");
        }
    }

    /// AC1 — single-file layout (the default) creates one `engram_data.db` and
    /// the per-store file names do not appear. Mirrors the agentzero adapter
    /// invariant (`agentzero/.../persistence_factory.rs` single-file assertion).
    #[test]
    fn open_provider_produces_single_file_by_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = test_config(dir.path());
        let _provider = open_provider(&config).expect("provider opens single-file");

        assert!(
            dir.path().join("engram_data.db").exists(),
            "single-file layout should create engram_data.db"
        );
        for per_store in [
            "memory.db",
            "knowledge.db",
            "belief.db",
            "hierarchy.db",
            "vectors.db",
        ] {
            assert!(
                !dir.path().join(per_store).exists(),
                "{per_store} should be folded into engram_data.db in single-file mode"
            );
        }
    }
}
