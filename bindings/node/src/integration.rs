//! Integration facade exposed to Node through N-API.
//!
//! This module bridges the Rust integration contract
//! (`engram_conformance::bootstrap_provider`) to TypeScript as a JSON-in /
//! JSON-out function, without changing the existing per-family engine surface.
//! TypeScript callers pass a serialized [`EngramConfig`] and receive a
//! capability report plus schema/adapter versions.

use engram_conformance::bootstrap_provider;
use engram_integration::EngramConfig;
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::to_napi_error;

/// Result of bootstrapping the integration provider.
///
/// Returned to Node as a JSON string so the TypeScript layer can validate and
/// consume the capability report without the binding re-implementing
/// capability logic.
#[napi(object)]
pub struct IntegrationBootstrapResult {
    /// JSON-encoded capability report (one entry per family with state + reason).
    pub capabilities_json: String,
    /// Storage schema version visible through provider diagnostics.
    pub schema_version: String,
    /// Adapter version visible through provider diagnostics.
    pub adapter_version: String,
}

/// Bootstraps a fully-wired integration provider from a JSON-encoded config.
///
/// The config JSON must match the v1 `EngramConfig` shape (storage_path,
/// trusted_root, scope_mapping_strategy, embedding_provider, migration_mode,
/// capability_policy). The provider is constructed, every capability family's
/// conformance fixture is run, and handles are attached only where the fixture
/// passes.
///
/// Returns the capability report and diagnostic versions. The provider itself
/// is not currently held across the boundary (each call constructs one); this
/// entry point exists to expose capability discovery and the integration
/// contract to TypeScript. Long-lived provider state remains accessible through
/// the existing per-family engines.
#[napi(js_name = "bootstrapIntegrationProvider")]
pub fn bootstrap_integration_provider(config_json: String) -> Result<IntegrationBootstrapResult> {
    let config: EngramConfig = serde_json::from_str(&config_json).map_err(|e| {
        to_napi_error(engram_runtime::CoreError::InvalidRequest {
            reason: format!("invalid integration config json: {e}"),
        })
    })?;

    let provider = bootstrap_provider(&config).map_err(to_napi_error)?;

    let report = provider.capabilities();
    let capabilities_json = serde_json::to_string(report).map_err(|e| {
        to_napi_error(engram_runtime::CoreError::Adapter {
            adapter: "engram-node.integration".to_string(),
            message: format!("serialize capability report: {e}"),
        })
    })?;

    Ok(IntegrationBootstrapResult {
        capabilities_json,
        schema_version: provider.schema_version().to_string(),
        adapter_version: provider.adapter_version().to_string(),
    })
}
