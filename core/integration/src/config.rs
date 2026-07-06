//! Provider configuration for the Engram integration facade.
//!
//! This module defines the configuration contract that host applications
//! use to bootstrap the Engram provider with explicit storage paths,
//! embedding providers, and capability/migration policies.

use engram_domain::types::ScopeMappingStrategy;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Capability policy determines how unsupported capabilities are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityPolicy {
    /// Fail closed — return errors for unsupported capabilities.
    /// This is the default and recommended mode for production use.
    FailClosed,

    /// Omit unsupported capabilities from the provider facade.
    /// Applications must check capability reports before using features.
    OmitUnsupported,
}

/// Migration mode controls whether migration operations are dry-run or applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationMode {
    /// Dry-run mode — validate and report without applying changes.
    /// This is the default and recommended mode for production use.
    DryRun,

    /// Apply mode — execute migration operations after validation.
    Apply,
}

/// Embedding provider configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingProviderConfig {
    /// Provider type (e.g., "fastembed", "ollama", "openai").
    pub provider_type: String,

    /// Model identifier within the provider (e.g., "BAAI/bge-small-en-v1.5").
    pub model: String,

    /// Vector dimensions for the model (e.g., 384).
    pub dimensions: u32,

    /// Prompt profile for embedding generation (e.g., "query", "passage").
    pub prompt_profile: String,

    /// Normalization applied to embeddings (e.g., "none", "l2", "cosine").
    pub normalization: Option<String>,
}

/// Engram provider configuration.
///
/// This configuration defines the storage path, trusted root, scope policy,
/// embedding provider, and capability/migration policies that control how
/// the Engram provider behaves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngramConfig {
    /// Path to the storage directory where all data is persisted.
    pub storage_path: PathBuf,

    /// Trusted root directory for path confinement validation.
    /// All storage paths must be within this root.
    pub trusted_root: PathBuf,

    /// Scope mapping strategy for policy enforcement.
    pub scope_policy: ScopeMappingStrategy,

    /// Embedding provider configuration for vector operations.
    pub embedding_provider: EmbeddingProviderConfig,

    /// Migration mode controlling whether migrations are dry-run or applied.
    pub migration_mode: MigrationMode,

    /// Capability policy determining how unsupported capabilities are handled.
    pub capability_policy: CapabilityPolicy,
}

impl EngramConfig {
    /// Creates a new Engram configuration with the given parameters.
    pub fn new(
        storage_path: impl Into<PathBuf>,
        trusted_root: impl Into<PathBuf>,
        scope_policy: ScopeMappingStrategy,
        embedding_provider: EmbeddingProviderConfig,
        migration_mode: MigrationMode,
        capability_policy: CapabilityPolicy,
    ) -> Self {
        Self {
            storage_path: storage_path.into(),
            trusted_root: trusted_root.into(),
            scope_policy,
            embedding_provider,
            migration_mode,
            capability_policy,
        }
    }

    /// Validates the configuration for correctness and security.
    ///
    /// Returns an error if:
    /// - storage_path is empty
    /// - trusted_root is missing or does not exist
    /// - storage_path is outside trusted_root (path traversal)
    /// - storage_path is a symlink pointing outside trusted_root
    pub fn validate(&self) -> Result<(), String> {
        // Check storage_path is not empty
        if self.storage_path.as_os_str().is_empty() {
            return Err("storage_path cannot be empty".to_string());
        }

        // Check trusted_root exists
        if !self.trusted_root.exists() {
            return Err(format!(
                "trusted_root does not exist: {:?}",
                self.trusted_root
            ));
        }

        // Resolve storage_path to its canonical form (follows symlinks)
        let storage_path = match std::fs::canonicalize(&self.storage_path) {
            Ok(path) => path,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Path doesn't exist yet — check parent directory
                let parent = match self.storage_path.parent() {
                    Some(p) => p,
                    None => return Err("storage_path has no parent directory".to_string()),
                };

                // Resolve parent to canonical form
                let parent_canonical = match std::fs::canonicalize(parent) {
                    Ok(p) => p,
                    Err(e) => return Err(format!("cannot resolve storage_path parent: {}", e)),
                };

                // Simulate the full path by joining parent with filename
                match self.storage_path.file_name() {
                    Some(filename) => parent_canonical.join(filename),
                    None => return Err("storage_path has no file name".to_string()),
                }
            }
            Err(e) => return Err(format!("cannot resolve storage_path: {}", e)),
        };

        // Resolve trusted_root to canonical form
        let trusted_root = match std::fs::canonicalize(&self.trusted_root) {
            Ok(path) => path,
            Err(e) => return Err(format!("cannot resolve trusted_root: {}", e)),
        };

        // Check storage_path starts with trusted_root (no path traversal)
        if !storage_path.starts_with(&trusted_root) {
            return Err(format!(
                "storage_path {:?} is outside trusted_root {:?}",
                storage_path, trusted_root
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_engram_config_serialization() {
        let config = EngramConfig::new(
            "/tmp/engram",
            "/tmp",
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

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: EngramConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.storage_path, config.storage_path);
        assert_eq!(deserialized.capability_policy, config.capability_policy);
    }

    #[test]
    fn test_capability_policy_modes() {
        let fail_closed = CapabilityPolicy::FailClosed;
        let omit = CapabilityPolicy::OmitUnsupported;

        assert_ne!(fail_closed, omit);
    }

    #[test]
    fn test_migration_mode_enforcement() {
        let dry_run = MigrationMode::DryRun;
        let apply = MigrationMode::Apply;

        assert_ne!(dry_run, apply);
    }

    #[test]
    fn test_config_validation_rejects_empty_storage_path() {
        let temp_dir = TempDir::new().unwrap();
        let config = EngramConfig::new(
            "",
            temp_dir.path(),
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

        let result = config.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "storage_path cannot be empty");
    }

    #[test]
    fn test_config_validation_rejects_missing_trusted_root() {
        let config = EngramConfig::new(
            "/tmp/engram",
            "/nonexistent/path",
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

        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_rejects_path_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let trusted_root = temp_dir.path();

        // Try to escape trusted_root via ../
        let storage_path = trusted_root.join("../escape");
        let config = EngramConfig::new(
            storage_path,
            trusted_root,
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

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside trusted_root"));
    }

    #[test]
    fn test_config_validation_accepts_valid_config() {
        let temp_dir = TempDir::new().unwrap();
        let trusted_root = temp_dir.path();
        let storage_path = trusted_root.join("engram");

        let config = EngramConfig::new(
            &storage_path,
            trusted_root,
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

        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_validation_rejects_symlink_escape() {
        let temp_dir = TempDir::new().unwrap();
        let trusted_root = temp_dir.path();
        let outside_dir = TempDir::new().unwrap();

        // Create a symlink inside trusted_root that points outside
        let symlink_path = trusted_root.join("symlink_escape");
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside_dir.path(), &symlink_path).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(outside_dir.path(), &symlink_path).unwrap();

        let storage_path = symlink_path.join("engram");
        let config = EngramConfig::new(
            &storage_path,
            trusted_root,
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

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside trusted_root"));
    }
}
