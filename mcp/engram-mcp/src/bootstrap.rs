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
    // RFC-0019 (Blocker 1): resolve the operator-facing `[recall_fusion]` config
    // through the discovery ladder and apply it to the `EngramConfig` before
    // `EngramProvider::open`. Without this, the MCP path always builds with
    // `recall_fusion = None` (the `EngramConfig::new` default) and unified
    // recall silently falls back to equal-weight RRF — the machinery exists
    // (`bootstrap_sqlite` reads `config.recall_fusion`) but the MCP never fed
    // it. Resolve from the storage path like scan-filter discovery
    // (`codegraph::resolve_scan_filter`): `<storage_path>/.engram/recall.json`
    // is rung 2 of the ladder; rung 1 (an explicit `[recall_fusion]` profile
    // section) would require a profile path on `McpConfig`, which v1 does not
    // carry — `open_provider` builds the config from flags, not a profile file.
    //
    // A present-but-invalid file surfaces as a boot error (not a silent
    // swallow): the operator wrote a config expecting weighted fusion; a
    // silent fall-back to equal-weight would hide the malformed file until
    // someone noticed recall "feels wrong". An absent file is the
    // backward-compatible equal-weight default (`Ok(None)`).
    let engram_config = match EngramConfig::discover_recall_fusion(&config.storage_path) {
        Ok(Some(fusion)) => engram_config.with_recall_fusion(fusion),
        Ok(None) => engram_config,
        Err(message) => {
            return Err(format!(
                "failed to load recall fusion config from storage path: {message}"
            ));
        }
    };
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

    // ---- RFC-0019 Blocker 1: MCP loads [recall_fusion] --------------------
    //
    // The bug this guards: `open_provider` built `EngramConfig` without ever
    // applying `.with_recall_fusion(...)`, so `config.recall_fusion` was always
    // `None` and the MCP path silently fell back to equal-weight RRF — the
    // machinery in `bootstrap_sqlite` existed but was never fed. The strongest
    // feasible proof at the MCP boundary is the boot-error path: a present,
    // *invalid* `.engram/recall.json` must surface a boot error. Without the
    // fix, the file is ignored and `open_provider` succeeds (equal-weight
    // default) — so this test fails on the bug and passes on the fix. That
    // validation only runs when the config is actually loaded is what makes it
    // a weighted-fusion-active signal, not an equal-weight-default one.

    fn write_recall_json(dir: &std::path::Path, body: &str) {
        let engram_dir = dir.join(".engram");
        std::fs::create_dir_all(&engram_dir).expect("create .engram dir");
        std::fs::write(engram_dir.join("recall.json"), body).expect("write recall.json");
    }

    /// Regression: an invalid `.engram/recall.json` (here `rrf_k = 0`) must
    /// surface as a boot error. On the Blocker-1 bug the file was ignored and
    /// this call succeeded with equal-weight fusion — so this assertion is the
    /// mechanical proof the MCP provider path reads and validates the config.
    #[test]
    fn open_provider_invalid_recall_fusion_is_boot_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_recall_json(dir.path(), r#"{"rrf_k":0}"#);
        let config = test_config(dir.path());
        // `EngramProvider` is not `Debug`, so `expect_err` is unavailable —
        // match by hand. `Ok` here is the bug signal (file was ignored).
        match open_provider(&config) {
            Err(err) => assert!(
                err.contains("recall fusion") || err.contains("recall.json"),
                "error should reference the recall fusion config: {err}"
            ),
            Ok(_) => panic!(
                "invalid recall.json must boot-error, but provider opened (config was ignored — Blocker 1 regression)"
            ),
        }
    }

    /// A valid weighted `.engram/recall.json` is applied (the provider opens
    /// with the weighted config). The weighted-fusion-active half of the chain
    /// (that `SqlUnifiedRecall` honors the weights) is covered by the
    /// integration-level recall tests; this test proves the MCP feeder.
    #[test]
    fn open_provider_loads_valid_recall_fusion_from_engram_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A weighted config biased toward vector + lexical (zbot-style). Valid:
        // k >= 1, weights finite & >= 0, keys from the documented vocabulary.
        write_recall_json(
            dir.path(),
            r#"{"rrf_k":42,"default_source_weight":1.0,"source_weights":{"vector":0.7,"lexical":0.3}}"#,
        );
        let config = test_config(dir.path());
        let provider = open_provider(&config).expect("valid recall.json opens through MCP");
        // The recall handle is wired under sqlite; its fusion now carries the
        // operator's weights rather than the equal-weight default.
        assert!(provider.recall().is_some(), "recall handle wired");
    }

    /// Backward compatibility: no `.engram/recall.json` ⇒ equal-weight default
    /// ⇒ the provider still opens (no config is forced on operators).
    #[test]
    fn open_provider_absent_recall_fusion_opens_equal_weight() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Intentionally do NOT write `.engram/recall.json`.
        let config = test_config(dir.path());
        let provider =
            open_provider(&config).expect("absent recall.json opens with equal-weight default");
        assert!(provider.recall().is_some(), "recall handle wired");
    }

    /// A malformed (unparseable) `.engram/recall.json` also surfaces as a boot
    /// error — the read/parse failure path, not just the validation path.
    #[test]
    fn open_provider_malformed_recall_fusion_is_boot_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_recall_json(dir.path(), "{ not json");
        let config = test_config(dir.path());
        match open_provider(&config) {
            Err(err) => assert!(
                err.contains("recall fusion") || err.contains("recall.json"),
                "error should reference the recall fusion config: {err}"
            ),
            Ok(_) => panic!(
                "malformed recall.json must boot-error, but provider opened (config was ignored — Blocker 1 regression)"
            ),
        }
    }
}
