//! Surreal backend wiring: construct the embedded SurrealKV store from
//! configuration and return a wired provider.
//!
//! ADR-0022 (amended 2026-07-16): this module names `Surreal*` / holds the
//! engine adapters by design and is intentionally exempt from the
//! engine-neutrality gate.

use crate::{CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder};
use engram_domain::{CapabilityReason, CapabilityState};
use engram_runtime::CoreResult;

/// Bootstraps a fully-wired provider from configuration against the Surreal
/// backend (embedded SurrealKV).
///
/// v1 foundation: constructs the provider and a Surreal capability report.
/// The capability cells (memory, knowledge, belief, hierarchy, vectors,
/// consolidation) are wired in subsequent increments — memory first (plan T4).
/// Until a cell ships, its capability reports `Unsupported` so the facade fails
/// closed rather than silently no-op-ing, matching the backend-parametric
/// conformance contract (ADR-0022 rule 4).
pub(crate) fn bootstrap_surreal(config: &EngramConfig) -> CoreResult<EngramProvider> {
    // v1 foundation: the Surreal backend is selected and constructs a valid
    // provider. `config.storage_path` is the embedded SurrealKV root (set from
    // `BackendProfile::Surreal { data_root }`). Capability handles attach as
    // their cells land; until then every capability is `ProviderUnavailable`.
    let _ = config;
    let report = CapabilityReport::new(CapabilityState::Unsupported {
        reason: CapabilityReason::ProviderUnavailable,
    });
    Ok(EngramProviderBuilder::new(report).build())
}

#[cfg(test)]
mod tests {
    //! The Surreal bootstrap path is feature-gated; these tests compile and run
    //! only with `--features surreal`. They prove the backend is selectable and
    //! constructs a valid provider — the "easy switch between providers" gate
    //! (plan T3). Real capability parity lands with the cells (T4-T6).
    use super::*;
    use engram_domain::types::ScopeMappingStrategy;
    use tempfile::TempDir;

    fn test_config(dir: &TempDir) -> EngramConfig {
        EngramConfig::new(
            dir.path().join("surreal"),
            dir.path(),
            ScopeMappingStrategy::Strict,
            crate::EmbeddingProviderConfig {
                provider_type: "fastembed".to_string(),
                model: "m".to_string(),
                dimensions: 384,
                prompt_profile: "query".to_string(),
                normalization: None,
            },
            crate::MigrationMode::DryRun,
            crate::CapabilityPolicy::FailClosed,
        )
    }

    #[test]
    fn bootstrap_surreal_constructs_provider() {
        let dir = TempDir::new().unwrap();
        let provider = bootstrap_surreal(&test_config(&dir)).expect("surreal bootstrap");
        // v1 foundation: provider constructs; capabilities Unsupported until the
        // memory cell (T4) wires a real SurrealMemoryService handle.
        assert!(
            !provider.capabilities().memory.is_supported(),
            "memory must be Unsupported until the Surreal memory cell ships"
        );
    }
}
