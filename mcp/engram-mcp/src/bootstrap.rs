//! Open the [`EngramProvider`] that every tool routes through.
//!
//! This is the single place the server touches a backend: it builds an
//! [`EngramConfig`] (embedding-`none`, SQLite) and calls
//! [`EngramProvider::open`]. No tool opens a store directly.

use engram_integration::{EngramConfig, EngramProvider};

use crate::config::McpConfig;

/// Open the provider described by `config`. Errors are surfaced as a string
/// for the caller (`main`) to report.
pub fn open_provider(config: &McpConfig) -> Result<EngramProvider, String> {
    let engram_config = EngramConfig::new(
        config.storage_path.clone(),
        std::env::temp_dir(),
        config.scope_strategy,
        config.embedding.clone(),
        config.migration_mode,
        config.capability_policy,
    );
    EngramProvider::open(&engram_config).map_err(|e| format!("failed to open engram provider: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::ScopeMappingStrategy;
    use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, MigrationMode};

    fn test_config(dir: &std::path::Path) -> McpConfig {
        McpConfig {
            storage_path: dir.join("engram_data.db"),
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
}
