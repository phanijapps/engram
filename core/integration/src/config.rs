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

/// SQLite storage layout for the provider's backing databases.
///
/// Controls whether each store opens its own file under `storage_path`
/// (`memory.db`, `knowledge.db`, …) or every SQLite-backed store shares one
/// file. The store schemas use disjoint table names, so a single file holds
/// memory, knowledge, belief, hierarchy, and vector tables side by side
/// without collisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[derive(Default)]
pub enum SqliteStorageLayout {
    /// One database file per store (the default; backward compatible).
    /// Creates `memory.db`, `knowledge.db`, `belief.db`, `hierarchy.db`, and
    /// `vectors.db` under `storage_path`.
    #[default]
    MultiFileDirectory,

    /// All SQLite-backed stores open the same file under `storage_path`.
    /// Useful for desktop/local-first hosts that prefer one file (plus its
    /// `-wal`/`-shm` sidecars) for backup, debug, and delete simplicity.
    SingleFile {
        /// Bare file name for the shared database, e.g. `"engram_data.db"`.
        /// Validated to be a single path component with a `.db`/`.sqlite`/
        /// `.sqlite3` extension — no separators, no `..`, no drive letters.
        file_name: String,
    },
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

    /// SQLite storage layout (multi-file directory by default; opt-in single
    /// file). Defaults via `#[serde(default)]` so existing configs without the
    /// field deserialize to `MultiFileDirectory`.
    #[serde(default)]
    pub sqlite_storage_layout: SqliteStorageLayout,
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
            sqlite_storage_layout: SqliteStorageLayout::MultiFileDirectory,
        }
    }

    /// Builder for opting into the single-file SQLite layout.
    ///
    /// ```ignore
    /// let config = EngramConfig::new(/* ... */)
    ///     .with_sqlite_storage_layout(SqliteStorageLayout::SingleFile {
    ///         file_name: "engram_data.db".to_string(),
    ///     });
    /// ```
    #[must_use]
    pub fn with_sqlite_storage_layout(mut self, layout: SqliteStorageLayout) -> Self {
        self.sqlite_storage_layout = layout;
        self
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

        // Validate the single-file layout's file_name before any path is built
        // from it. A bare, validated name guarantees storage_path.join(name)
        // stays within the trusted root, so the storage_path confinement check
        // below also covers the shared database path.
        if let SqliteStorageLayout::SingleFile { file_name } = &self.sqlite_storage_layout {
            validate_single_file_name(file_name)?;
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

/// Validates a single-file layout `file_name`.
///
/// Guarantees `storage_path.join(file_name)` stays inside the trusted root: a
/// bare name with no separators, no `..`, and no drive letter cannot escape.
fn validate_single_file_name(file_name: &str) -> Result<(), String> {
    if file_name.trim().is_empty() {
        return Err("single-file layout file_name cannot be empty".to_string());
    }
    if file_name.contains('/') || file_name.contains('\\') {
        return Err("single-file layout file_name must not contain path separators".to_string());
    }
    // `..` (exact) escapes via join; `.` is a directory, not a file name.
    if file_name == ".." || file_name == "." {
        return Err(format!(
            "single-file layout file_name must be a real file name, not '{file_name}'"
        ));
    }
    // Reject Windows drive-relative names like "C:foo".
    let bytes = file_name.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Err(
            "single-file layout file_name must not be an absolute or drive path".to_string(),
        );
    }
    let lower = file_name.to_ascii_lowercase();
    let has_valid_ext =
        lower.ends_with(".db") || lower.ends_with(".sqlite") || lower.ends_with(".sqlite3");
    if !has_valid_ext {
        return Err(
            "single-file layout file_name must end in .db, .sqlite, or .sqlite3".to_string(),
        );
    }
    Ok(())
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

    fn single_file_config(file_name: &str, trusted_root: &std::path::Path) -> EngramConfig {
        EngramConfig::new(
            trusted_root.join("engram"),
            trusted_root,
            ScopeMappingStrategy::Strict,
            EmbeddingProviderConfig {
                provider_type: "fastembed".to_string(),
                model: "bge-small-en-v1.5".to_string(),
                dimensions: 384,
                prompt_profile: "query".to_string(),
                normalization: None,
            },
            MigrationMode::DryRun,
            CapabilityPolicy::FailClosed,
        )
        .with_sqlite_storage_layout(SqliteStorageLayout::SingleFile {
            file_name: file_name.to_string(),
        })
    }

    #[test]
    fn single_file_layout_accepts_valid_file_name() {
        let temp_dir = TempDir::new().unwrap();
        let config = single_file_config("engram_data.db", temp_dir.path());
        assert!(config.validate().is_ok(), "{:?}", config.validate());
    }

    #[test]
    fn single_file_layout_rejects_empty_file_name() {
        let temp_dir = TempDir::new().unwrap();
        let err = single_file_config("  ", temp_dir.path())
            .validate()
            .unwrap_err();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn single_file_layout_rejects_path_separators() {
        let temp_dir = TempDir::new().unwrap();
        for bad in ["evil/x.db", "evil\\x.db", "../escape.db", "a/../b.db"] {
            let err = single_file_config(bad, temp_dir.path())
                .validate()
                .unwrap_err();
            assert!(
                err.contains("separator") || err.contains(".."),
                "accepted {bad}: {err}"
            );
        }
    }

    #[test]
    fn single_file_layout_rejects_directory_names() {
        let temp_dir = TempDir::new().unwrap();
        for bad in [".", ".."] {
            let err = single_file_config(bad, temp_dir.path())
                .validate()
                .unwrap_err();
            assert!(err.contains("real file name"), "accepted {bad}: {err}");
        }
    }

    #[test]
    fn single_file_layout_rejects_drive_paths() {
        let temp_dir = TempDir::new().unwrap();
        let err = single_file_config("C:engram.db", temp_dir.path())
            .validate()
            .unwrap_err();
        assert!(err.contains("absolute or drive"), "{err}");
    }

    #[test]
    fn single_file_layout_rejects_bad_extension() {
        let temp_dir = TempDir::new().unwrap();
        let err = single_file_config("engram_data.txt", temp_dir.path())
            .validate()
            .unwrap_err();
        assert!(err.contains("must end in"), "{err}");
    }

    #[test]
    fn single_file_layout_rejects_traversal_file_name() {
        // A bare ".." would join to the parent of storage_path and escape the
        // trusted root. The validator must reject it before any path is built.
        let temp_dir = TempDir::new().unwrap();
        let err = single_file_config("..", temp_dir.path())
            .validate()
            .unwrap_err();
        assert!(err.contains("real file name"), "{err}");
    }

    #[test]
    fn default_layout_is_multi_file() {
        // new() without with_sqlite_storage_layout must default to multi-file so
        // existing configs/hosts are unaffected.
        let temp_dir = TempDir::new().unwrap();
        let config = EngramConfig::new(
            temp_dir.path().join("engram"),
            temp_dir.path(),
            ScopeMappingStrategy::Strict,
            EmbeddingProviderConfig {
                provider_type: "fastembed".to_string(),
                model: "m".to_string(),
                dimensions: 384,
                prompt_profile: "query".to_string(),
                normalization: None,
            },
            MigrationMode::DryRun,
            CapabilityPolicy::FailClosed,
        );
        assert_eq!(
            config.sqlite_storage_layout,
            SqliteStorageLayout::MultiFileDirectory
        );
    }

    #[test]
    fn single_file_layout_round_trips_through_serde_with_default() {
        // An existing JSON config without the layout field must deserialize to
        // the default multi-file layout (backward compatibility).
        let temp_dir = TempDir::new().unwrap();
        let config = EngramConfig::new(
            temp_dir.path().join("engram"),
            temp_dir.path(),
            ScopeMappingStrategy::Strict,
            EmbeddingProviderConfig {
                provider_type: "fastembed".to_string(),
                model: "m".to_string(),
                dimensions: 384,
                prompt_profile: "query".to_string(),
                normalization: None,
            },
            MigrationMode::DryRun,
            CapabilityPolicy::FailClosed,
        );
        let json = serde_json::to_string(&config).unwrap();
        // Strip the layout field to simulate a pre-existing config.
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("sqlite_storage_layout");
        let legacy = serde_json::to_string(&value).unwrap();
        let parsed: EngramConfig = serde_json::from_str(&legacy).unwrap();
        assert_eq!(
            parsed.sqlite_storage_layout,
            SqliteStorageLayout::MultiFileDirectory
        );
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
