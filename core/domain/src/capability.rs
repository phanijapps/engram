//! Capability reporting types for the integration contract.
//!
//! This module defines the structured capability state that applications
//! discover at bootstrap time, with stable reason codes for unsupported,
//! degraded, or misconfigured states.

use serde::{Deserialize, Serialize};

/// Stable reason codes for capability state explanations.
///
/// These string constants are part of the stable integration contract —
/// once released, they remain stable for compatibility. New reasons may be
/// added, but existing reasons are never removed or changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityReason {
    /// Provider is unavailable or cannot be initialized.
    ProviderUnavailable,

    /// Embedding space (provider + model + dimensions + profile) does not match.
    EmbeddingSpaceMismatch,

    /// Vector dimensions do not match the expected size.
    DimensionMismatch,

    /// Record-time history is not supported by this adapter.
    RecordTimeHistoryUnsupported,

    /// Migration manifest is stale (fingerprint mismatch).
    MigrationManifestStale,

    /// Storage path is outside the trusted root.
    StoragePathOutsideTrustedRoot,

    /// Store family is not supported by this adapter.
    UnsupportedStoreFamily,

    /// Conformance fixture failed.
    ConformanceFailed,

    /// Feature is disabled (not enabled in configuration).
    FeatureDisabled,
}

impl CapabilityReason {
    /// Convert to the stable string representation used in diagnostics and APIs.
    pub fn as_str(&self) -> &'static str {
        match self {
            CapabilityReason::ProviderUnavailable => "provider_unavailable",
            CapabilityReason::EmbeddingSpaceMismatch => "embedding_space_mismatch",
            CapabilityReason::DimensionMismatch => "dimension_mismatch",
            CapabilityReason::RecordTimeHistoryUnsupported => "record_time_history_unsupported",
            CapabilityReason::MigrationManifestStale => "migration_manifest_stale",
            CapabilityReason::StoragePathOutsideTrustedRoot => "storage_path_outside_trusted_root",
            CapabilityReason::UnsupportedStoreFamily => "unsupported_store_family",
            CapabilityReason::ConformanceFailed => "conformance_failed",
            CapabilityReason::FeatureDisabled => "feature_disabled",
        }
    }
}

/// Capability state for one feature family.
///
/// Applications use this state to decide which features are safe to enable
/// before starting workers, routes, tools, or retrieval paths. The state is
/// machine-readable and stable; the reason codes explain why a feature is
/// not supported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityState {
    /// Feature is fully supported and ready to use.
    Supported,

    /// Feature is not supported with a stable reason code.
    Unsupported {
        /// Why the feature is not supported.
        reason: CapabilityReason,
    },

    /// Feature is supported but in a degraded mode (reduced performance,
    /// limited functionality, or other constraints) with a stable reason code.
    Degraded {
        /// Why the feature is degraded.
        reason: CapabilityReason,
    },

    /// Feature requires a storage migration before it can be supported.
    RequiresMigration {
        /// What migration is needed.
        reason: CapabilityReason,
    },

    /// Feature requires a vector re-index before it can be supported.
    RequiresReindex {
        /// Why reindexing is required (usually embedding space change).
        reason: CapabilityReason,
    },

    /// Feature is supported but misconfigured (invalid path, missing dependency,
    /// or other correctable issue).
    Misconfigured {
        /// What is misconfigured.
        reason: CapabilityReason,
    },
}

impl CapabilityState {
    /// Returns true if the feature is fully supported and ready to use.
    pub fn is_supported(&self) -> bool {
        matches!(self, CapabilityState::Supported)
    }

    /// Returns the reason code if the state is not Supported.
    pub fn reason(&self) -> Option<&str> {
        match self {
            CapabilityState::Supported => None,
            CapabilityState::Unsupported { reason } => Some(reason.as_str()),
            CapabilityState::Degraded { reason } => Some(reason.as_str()),
            CapabilityState::RequiresMigration { reason } => Some(reason.as_str()),
            CapabilityState::RequiresReindex { reason } => Some(reason.as_str()),
            CapabilityState::Misconfigured { reason } => Some(reason.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_state_serialization() {
        // Test Supported state
        let state = CapabilityState::Supported;
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: CapabilityState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, state);
        assert!(deserialized.is_supported());
        assert!(deserialized.reason().is_none());

        // Test Unsupported state with reason
        let state = CapabilityState::Unsupported {
            reason: CapabilityReason::ProviderUnavailable,
        };
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: CapabilityState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, state);
        assert!(!deserialized.is_supported());
        assert_eq!(deserialized.reason(), Some("provider_unavailable"));
    }

    #[test]
    fn test_capability_reason_stability() {
        // Verify all reason codes have stable string representations
        assert_eq!(
            CapabilityReason::ProviderUnavailable.as_str(),
            "provider_unavailable"
        );
        assert_eq!(
            CapabilityReason::EmbeddingSpaceMismatch.as_str(),
            "embedding_space_mismatch"
        );
        assert_eq!(
            CapabilityReason::DimensionMismatch.as_str(),
            "dimension_mismatch"
        );
        assert_eq!(
            CapabilityReason::RecordTimeHistoryUnsupported.as_str(),
            "record_time_history_unsupported"
        );
        assert_eq!(
            CapabilityReason::MigrationManifestStale.as_str(),
            "migration_manifest_stale"
        );
        assert_eq!(
            CapabilityReason::StoragePathOutsideTrustedRoot.as_str(),
            "storage_path_outside_trusted_root"
        );
        assert_eq!(
            CapabilityReason::UnsupportedStoreFamily.as_str(),
            "unsupported_store_family"
        );
        assert_eq!(
            CapabilityReason::ConformanceFailed.as_str(),
            "conformance_failed"
        );
        assert_eq!(
            CapabilityReason::FeatureDisabled.as_str(),
            "feature_disabled"
        );
    }

    #[test]
    fn test_capability_state_reason_extraction() {
        // Test that reason extraction works for all non-Supported states
        let unsupported = CapabilityState::Unsupported {
            reason: CapabilityReason::DimensionMismatch,
        };
        assert_eq!(unsupported.reason(), Some("dimension_mismatch"));

        let degraded = CapabilityState::Degraded {
            reason: CapabilityReason::EmbeddingSpaceMismatch,
        };
        assert_eq!(degraded.reason(), Some("embedding_space_mismatch"));

        let requires_migration = CapabilityState::RequiresMigration {
            reason: CapabilityReason::MigrationManifestStale,
        };
        assert_eq!(
            requires_migration.reason(),
            Some("migration_manifest_stale")
        );

        let requires_reindex = CapabilityState::RequiresReindex {
            reason: CapabilityReason::StoragePathOutsideTrustedRoot,
        };
        assert_eq!(
            requires_reindex.reason(),
            Some("storage_path_outside_trusted_root")
        );

        let misconfigured = CapabilityState::Misconfigured {
            reason: CapabilityReason::UnsupportedStoreFamily,
        };
        assert_eq!(misconfigured.reason(), Some("unsupported_store_family"));
    }
}
