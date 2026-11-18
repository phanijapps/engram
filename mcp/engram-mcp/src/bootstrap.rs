//! Single MCP opener: preflight once, then delegate to the selected provider factory.
use crate::config::McpConfig;
#[cfg(test)]
use crate::config::{McpBackend, McpSqliteLayout};
use engram_domain::{Policy, Requester, Scope};
use engram_integration::EngramProvider;
use engram_runtime::{CoreError, CoreResult, PolicyAuthorizer};
use std::sync::Arc;

/// Host authority is confined to the launch scope; canonical memory algorithms
/// remain responsible for allowed uses, retention, lifecycle and validation.
struct LaunchPolicy(Scope);
impl LaunchPolicy {
    fn authorize(&self, scope: &Scope) -> CoreResult<()> {
        if engram_memory::scope::scope_allows(scope, &self.0) {
            Ok(())
        } else {
            Err(CoreError::PolicyDenied {
                reason: "request scope is outside the MCP launch scope".into(),
            })
        }
    }
}
impl PolicyAuthorizer for LaunchPolicy {
    fn can_write(&self, _: &Requester, scope: &Scope, _: &Policy) -> CoreResult<()> {
        self.authorize(scope)
    }
    fn can_retrieve(&self, _: &Requester, scope: &Scope, _: &Policy) -> CoreResult<()> {
        self.authorize(scope)
    }
    fn can_forget(&self, _: &Requester, scope: &Scope, _: &Policy) -> CoreResult<()> {
        self.authorize(scope)
    }
}
/// No local storage/cache directory, model, credential or connection is opened
/// until the effective provider/mode/features and path configuration pass.
fn prepare_launch(config: &McpConfig) -> Result<(engram_integration::EngramConfig, Scope), String> {
    let mut effective = config.engram_config()?;
    effective =
        match engram_integration::EngramConfig::discover_recall_fusion(&effective.storage_path) {
            Ok(Some(fusion)) => effective.with_recall_fusion(fusion),
            Ok(None) => effective,
            Err(_) => return Err("failed to load recall fusion config from storage path".into()),
        };
    let launch_scope = crate::scope::resolve_scope(
        config.org.as_deref(),
        config.domain.as_deref(),
        config.subdomain.as_deref(),
        &config.project,
    );
    Ok((effective, launch_scope))
}
fn with_launch_authority(
    provider: EngramProvider,
    launch_scope: Scope,
) -> CoreResult<EngramProvider> {
    provider.with_code_authorizer(Arc::new(
        engram_integration::code_binding::ScopeSetAuthorizer::new(vec![launch_scope]),
    ))
}
pub fn open_provider(config: &McpConfig) -> Result<EngramProvider, String> {
    let (effective, launch_scope) = prepare_launch(config)?;
    engram_backend_provider::open(
        &effective,
        &launch_scope,
        Arc::new(LaunchPolicy(launch_scope.clone())),
    )
    .and_then(|provider| with_launch_authority(provider, launch_scope))
    .map_err(|error| error.to_redacted().to_string())
}

/// Only instrumented fixture builds expose this opener. The caller supplies
/// artifact identity; no launch flag or environment variable enables controls.
#[cfg(feature = "test-controls")]
#[allow(dead_code)]
pub(crate) fn open_instrumented_provider(
    config: &McpConfig,
    identity: engram_backend_provider::test_controls::FixtureIdentity,
) -> Result<engram_backend_provider::test_controls::InstrumentedProvider, String> {
    let (effective, launch_scope) = prepare_launch(config)?;
    let mut opened = engram_backend_provider::test_controls::open(
        &effective,
        &launch_scope,
        Arc::new(LaunchPolicy(launch_scope.clone())),
        identity,
    )
    .map_err(|error| error.to_redacted().to_string())?;
    opened.provider = with_launch_authority(opened.provider, launch_scope)
        .map_err(|error| error.to_redacted().to_string())?;
    Ok(opened)
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;
    use engram_domain::ScopeMappingStrategy;
    use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, MigrationMode};

    fn test_config(dir: &std::path::Path) -> McpConfig {
        McpConfig {
            trusted_root: None,
            helixdb: None,
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
                ..Default::default()
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
            // These MCP-level tests exercise the core/memory/knowledge/recall
            // handles and the single-file layout, not the vector lane. Disabling
            // vector keeps them fast and deterministic under the new fastembed
            // default (no FastEmbed model load attempt). The vector lane's
            // enable_vector gate is covered by the integration-crate bootstrap
            // tests directly.
            enable_vector: false,
            rebuild_vectors: false,
            tool_profile: String::new(),
            backend: McpBackend::Sqlite,
            pg_connection_string: None,
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
    /// (that the engine's unified recall honors the weights) is covered by the
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

    /// storage-dir-fix: a `--storage` path whose PARENT directories do not
    /// exist boots zero-config — the server creates the tree instead of
    /// failing validation with "trusted_root does not exist".
    #[test]
    fn open_provider_creates_missing_storage_tree() {
        let base = tempfile::tempdir().expect("tempdir");
        // Two levels that do not exist yet.
        let storage = base.path().join("deep").join("nested").join("store");
        let config = test_config(&storage);
        let provider =
            open_provider(&config).expect("boots with a missing storage tree (creates it)");
        let _ = provider.capabilities();
        assert!(
            storage.is_dir(),
            "storage tree created: {}",
            storage.display()
        );
    }
}
