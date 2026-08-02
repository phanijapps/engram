//! Provider configuration for the Engram integration facade.
//!
//! This module defines the configuration contract that host applications
//! use to bootstrap the Engram provider with explicit storage paths,
//! embedding providers, and capability/migration policies.

use engram_domain::types::ScopeMappingStrategy;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Serde default for `EngramConfig::enable_vector` — `true`. Kept as a named
/// fn (rather than `#[serde(default = "default_true")]` inline) so the field
/// deserializes to `true` when omitted from a legacy config file (backward
/// compatibility: a deployment that never set the field keeps vector wiring on
/// when the `fastembed` feature is compiled in).
fn default_true() -> bool {
    true
}

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

/// Declarative backend selection for [`EngramConfig::from_profile_file`].
///
/// A profile file's `[backend]` section deserializes into this enum. The active
/// backend is `sqlite` (file-backed, behind the `sqlite` cargo feature); the
/// `postgres` and `surreal` variants are parsed and validated so profile files
/// stay forward-compatible, but opening a provider against them fails with
/// [`engram_runtime::CoreError::CapabilityUnsupported`] until the matching
/// backend feature ships.
///
/// Hosts select a backend by editing config, not by rewriting application code
/// (ADR-0022): swapping `sqlite` for `postgres` is a profile + feature-flag
/// change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BackendProfile {
    /// File-backed SQLite backend (the active default). `data_root` is the
    /// directory the SQLite database files live under.
    Sqlite {
        /// Directory holding the SQLite database files (`memory.db`,
        /// `knowledge.db`, …) or the single shared file.
        data_root: String,
    },
    /// Postgres backend (no active feature yet). Carries the connection string
    /// the future backend will consume.
    Postgres {
        /// Postgres connection string (e.g. `postgres://user@host/db`).
        connection_string: String,
    },
    /// Surreal backend (embedded, behind the `surreal` cargo feature). v1 is
    /// in-process; `data_root` is the directory the store lives under, mirroring
    /// the SQLite `data_root` shape. Select by compiling `--features surreal`
    /// and setting `[backend] kind = "surreal"`.
    Surreal {
        /// Directory holding the embedded Surreal store.
        data_root: String,
    },
}

/// Shape of a TOML profile file consumed by [`EngramConfig::from_profile_file`].
///
/// Every field except `backend` is optional and falls back to a conservative
/// default, so a minimal profile file need only declare its `[backend]`.
#[derive(Debug, Deserialize)]
struct ProfileFile {
    backend: BackendProfile,
    #[serde(default)]
    trusted_root: Option<PathBuf>,
    #[serde(default)]
    scope_policy: Option<ScopeMappingStrategy>,
    #[serde(default)]
    embedding_provider: Option<EmbeddingProviderConfig>,
    #[serde(default)]
    migration_mode: Option<MigrationMode>,
    #[serde(default)]
    capability_policy: Option<CapabilityPolicy>,
    #[serde(default)]
    sqlite_storage_layout: Option<SqliteStorageLayout>,
    /// Optional `[recall_fusion]` section (RFC-0019). An explicit profile
    /// section is the first rung of the discovery ladder; `.engram/recall.json`
    /// (see [`EngramConfig::discover_recall_fusion`]) is the second.
    #[serde(default)]
    recall_fusion: Option<engram_retrieval::RecallFusionConfig>,
    /// Optional runtime kill-switch for the vector lane (RFC-0019 D3 reversed).
    /// When `false`, `bootstrap_sqlite` skips constructing the vector index,
    /// the embedding provider/model, and the vector recall lane entirely — even
    /// when the `fastembed` feature is compiled in — so a deployment can avoid
    /// the model download/load without a rebuild. Defaults to `true` (omitted ⇒
    /// vector on when compiled in).
    #[serde(default)]
    enable_vector: Option<bool>,
}

/// The default embedding-provider config used when a profile file omits the
/// `[embedding_provider]` section.
fn default_embedding() -> EmbeddingProviderConfig {
    EmbeddingProviderConfig {
        provider_type: "fastembed".to_string(),
        model: "BAAI/bge-small-en-v1.5".to_string(),
        dimensions: 384,
        prompt_profile: "query".to_string(),
        normalization: None,
    }
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
///
/// `Eq` is intentionally NOT derived: `recall_fusion` carries `f32`
/// weights/lambda, and `f32` does not implement `Eq`. `PartialEq` remains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    /// Optional Postgres connection string for the pgvector backend. A config
    /// string (not an engine type), so it stays within ADR-0022's neutrality
    /// rule. `EngramProvider::open` rejects a config carrying this — it is
    /// engine-neutral and sqlite-default; open a pgvector provider through the
    /// `engram-backend-pgvector` recipe (`engram_backend_pgvector::open`).
    pub pgvector_connection_string: Option<String>,

    /// Optional external recall-fusion config (RFC-0019): RRF `k`, per-lane
    /// `source_weights`, and a reranker strategy. When `None`, unified recall
    /// falls back to equal-weight RRF (today's behavior). Loaded from a
    /// `[recall_fusion]` profile section or discovered at
    /// `<root>/.engram/recall.json`; validated on load via
    /// [`RecallFusionConfig::to_reciprocal_config`].
    #[serde(default)]
    pub recall_fusion: Option<engram_retrieval::RecallFusionConfig>,

    /// Runtime kill-switch for the vector lane (RFC-0019 D3 reversed). Defaults
    /// to `true`. When `false`, `bootstrap_sqlite` skips constructing the
    /// `SqliteVectorIndex`, the `FastEmbed` embedding provider/model, and the
    /// vector recall lane — even when the `fastembed` cargo feature is compiled
    /// in — so a deployment built with fastembed can still avoid the model
    /// download/load at boot. Build-time disable remains
    /// `--no-default-features`. The field is NOT behind a `#[cfg(feature =
    /// "fastembed")]`: it is always present (defaults `true`) so the config
    /// type is identical under both feature builds.
    #[serde(default = "default_true")]
    pub enable_vector: bool,
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
            pgvector_connection_string: None,
            recall_fusion: None,
            enable_vector: true,
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

    /// Opts into the Postgres (pgvector) backend with the given connection string.
    pub fn with_pgvector(mut self, connection_string: impl Into<String>) -> Self {
        self.pgvector_connection_string = Some(connection_string.into());
        self
    }

    /// Sets the external recall-fusion config (RFC-0019). When set, unified
    /// recall honors the configured RRF `k` + per-lane `source_weights` +
    /// reranker strategy; when absent (`None`), recall falls back to
    /// equal-weight RRF. The config is validated when unified recall builds
    /// its internal [`engram_retrieval::ReciprocalFusionConfig`].
    #[must_use]
    pub fn with_recall_fusion(mut self, fusion: engram_retrieval::RecallFusionConfig) -> Self {
        self.recall_fusion = Some(fusion);
        self
    }

    /// Sets the runtime vector kill-switch (RFC-0019 D3 reversed). `false`
    /// causes `bootstrap_sqlite` to skip the vector index, the embedding
    /// provider/model, and the vector recall lane at boot — even when the
    /// `fastembed` cargo feature is compiled in — so a fastembed build can
    /// avoid the model download/load. Defaults to `true` (see [`EngramConfig::new`]).
    #[must_use]
    pub fn with_enable_vector(mut self, enable: bool) -> Self {
        self.enable_vector = enable;
        self
    }

    /// Discovers a recall-fusion config from `<discovery_root>/.engram/recall.json`
    /// (RFC-0019 rung 2 of the ladder). This is the repo-local fallback when an
    /// explicit `[recall_fusion]` profile section is absent.
    ///
    /// Discovery mirrors the `scan.json` ladder in `engram-mcp::codegraph`:
    /// read directly (no `exists()` probe) so a transient removal between probe
    /// and read cannot produce a misleading error; `NotFound` simply means no
    /// config.
    ///
    /// # Errors
    ///
    /// - `Ok(None)` — no `recall.json` present (backward-compatible equal-weight
    ///   default applies).
    /// - `Ok(Some(cfg))` — a present, valid config (validated via
    ///   [`RecallFusionConfig::to_reciprocal_config`]).
    /// - `Err(message)` — the file exists but cannot be read, parsed, or
    ///   validated. The caller decides whether to abort or soft-fail; the MCP
    ///   bootstrap (`engram_mcp::bootstrap::open_provider`) treats `Err` as a
    ///   boot error so a malformed operator config surfaces at startup rather
    ///   than silently degrading to equal-weight recall. (An *absent* file is
    ///   `Ok(None)`, never `Err`.)
    pub fn discover_recall_fusion(
        discovery_root: &Path,
    ) -> Result<Option<engram_retrieval::RecallFusionConfig>, String> {
        let path = discovery_root.join(".engram").join("recall.json");
        let json = match std::fs::read_to_string(&path) {
            Ok(j) => j,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(format!("read {}: {e}", path.display()));
            }
        };
        let cfg: engram_retrieval::RecallFusionConfig =
            serde_json::from_str(&json).map_err(|e| format!("parse {}: {e}", path.display()))?;
        cfg.to_reciprocal_config()
            .map_err(|e| format!("invalid recall fusion config at {}: {e}", path.display()))?;
        Ok(Some(cfg))
    }

    /// Resolves the recall-fusion config via the discovery ladder
    /// (RFC-0019): an explicit value (e.g. a `[recall_fusion]` profile section
    /// already loaded) wins; otherwise `<discovery_root>/.engram/recall.json`
    /// is consulted; otherwise `None` (equal-weight default).
    ///
    /// A discovered file that fails to read/parse/validate is reported via
    /// `Err` but does NOT abort — callers that want the soft-fail semantics
    /// (matching `scan.json`) should treat `Err` as `None`.
    pub fn resolve_recall_fusion(
        explicit: Option<engram_retrieval::RecallFusionConfig>,
        discovery_root: &Path,
    ) -> Result<Option<engram_retrieval::RecallFusionConfig>, String> {
        if explicit.is_some() {
            return Ok(explicit);
        }
        Self::discover_recall_fusion(discovery_root)
    }

    /// Builds an [`EngramConfig`] from a TOML profile file.
    ///
    /// The file carries a `[backend]` section (a [`BackendProfile``]) that
    /// selects the storage backend declaratively, plus optional overrides for
    /// every other configuration field. Fields omitted from the file fall back
    /// to conservative defaults.
    ///
    /// For the active `sqlite` backend, `[backend]` looks like:
    ///
    /// ```toml
    /// [backend]
    /// kind = "sqlite"
    /// data_root = "/var/lib/engram"
    ///
    /// [embedding_provider]
    /// provider_type = "fastembed"
    /// model = "BAAI/bge-small-en-v1.5"
    /// dimensions = 384
    /// prompt_profile = "query"
    /// ```
    ///
    /// `data_root` becomes `storage_path`; `trusted_root` defaults to its parent
    /// directory. The `postgres` and `surreal` profile kinds are parsed and
    /// validated but have no active backend yet — opening a provider against them
    /// fails with [`engram_runtime::CoreError::CapabilityUnsupported`] until the
    /// matching backend feature ships.
    ///
    /// # Errors
    ///
    /// Returns a `String` describing the failure if the file cannot be read or
    /// the TOML does not deserialize into the profile shape.
    pub fn from_profile_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .map_err(|e| format!("read profile file {}: {e}", path.display()))?;
        let profile: ProfileFile = toml::from_str(&contents)
            .map_err(|e| format!("parse profile file {}: {e}", path.display()))?;

        let storage_path = match &profile.backend {
            BackendProfile::Sqlite { data_root } => PathBuf::from(data_root),
            BackendProfile::Postgres { connection_string } => {
                // No SQLite storage path for a Postgres profile; the active
                // backend feature is required to interpret the connection
                // string. Surface an explicit, typed message rather than a
                // confusing path-confinement failure.
                return Err(format!(
                    "backend `postgres` (connection_string={connection_string}) must be \
                     opened via the `engram-backend-pgvector` recipe \
                     (engram_backend_pgvector::open); EngramProvider::open is sqlite-default"
                ));
            }
            BackendProfile::Surreal { data_root } => PathBuf::from(data_root),
        };

        // trusted_root defaults to the parent of data_root so path confinement
        // holds; the parent must exist (validate() checks this).
        let trusted_root = profile.trusted_root.unwrap_or_else(|| {
            storage_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| storage_path.clone())
        });

        let embedding_provider = profile.embedding_provider.unwrap_or_else(default_embedding);
        // Validate an explicit `[recall_fusion]` section eagerly: a malformed
        // operator config should surface at load, not silently degrade to
        // equal-weight recall. Discovered `.engram/recall.json` files soft-fail
        // (see `discover_recall_fusion`); an explicit profile section does not.
        if let Some(ref fusion) = profile.recall_fusion
            && let Err(e) = fusion.to_reciprocal_config()
        {
            return Err(format!(
                "invalid [recall_fusion] section in profile {}: {e}",
                path.display()
            ));
        }
        Ok(Self {
            storage_path,
            trusted_root,
            scope_policy: profile.scope_policy.unwrap_or(ScopeMappingStrategy::Strict),
            embedding_provider,
            migration_mode: profile.migration_mode.unwrap_or(MigrationMode::DryRun),
            capability_policy: profile
                .capability_policy
                .unwrap_or(CapabilityPolicy::FailClosed),
            sqlite_storage_layout: profile
                .sqlite_storage_layout
                .unwrap_or(SqliteStorageLayout::MultiFileDirectory),
            pgvector_connection_string: None,
            recall_fusion: profile.recall_fusion,
            enable_vector: profile.enable_vector.unwrap_or(true),
        })
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

    // ---- RFC-0019 recall-fusion loading (T1c) -----------------------------

    use std::collections::BTreeMap;

    fn base_profile_toml(data_root: &str) -> String {
        format!("[backend]\nkind = \"sqlite\"\ndata_root = \"{data_root}\"\n")
    }

    #[test]
    fn profile_recall_fusion_section_loads_and_round_trips() {
        let temp_dir = TempDir::new().unwrap();
        let data_root = temp_dir.path().join("data");
        std::fs::create_dir_all(&data_root).unwrap();
        let toml = format!(
            "{base}\n[recall_fusion]\nrrf_k = 42\ndefault_source_weight = 1.0\n\n[recall_fusion.source_weights]\nvector = 0.7\nlexical = 0.3\n",
            base = base_profile_toml(data_root.to_str().unwrap())
        );
        let profile_path = temp_dir.path().join("profile.toml");
        std::fs::write(&profile_path, &toml).unwrap();

        let cfg = EngramConfig::from_profile_file(&profile_path).expect("profile loads");
        let fusion = cfg.recall_fusion.expect("recall_fusion section present");
        assert_eq!(fusion.rrf_k, 42);
        let recip = fusion.to_reciprocal_config().expect("valid");
        assert_eq!(recip.source_weight("vector"), 0.7);
        assert_eq!(recip.source_weight("lexical"), 0.3);
    }

    #[test]
    fn profile_without_recall_fusion_section_defaults_to_none() {
        // A profile without [recall_fusion] must load with recall_fusion = None
        // (backward-compatible equal-weight default).
        let temp_dir = TempDir::new().unwrap();
        let data_root = temp_dir.path().join("data");
        std::fs::create_dir_all(&data_root).unwrap();
        let toml = base_profile_toml(data_root.to_str().unwrap());
        let profile_path = temp_dir.path().join("profile.toml");
        std::fs::write(&profile_path, &toml).unwrap();

        let cfg = EngramConfig::from_profile_file(&profile_path).expect("profile loads");
        assert!(cfg.recall_fusion.is_none(), "absent section => None");
    }

    #[test]
    fn profile_recall_fusion_section_rejects_invalid_k() {
        // An explicit profile section is validated eagerly: k = 0 must surface
        // a load error rather than silently degrade.
        let temp_dir = TempDir::new().unwrap();
        let data_root = temp_dir.path().join("data");
        std::fs::create_dir_all(&data_root).unwrap();
        let toml = format!(
            "{base}\n[recall_fusion]\nrrf_k = 0\n",
            base = base_profile_toml(data_root.to_str().unwrap())
        );
        let profile_path = temp_dir.path().join("profile.toml");
        std::fs::write(&profile_path, &toml).unwrap();

        let err = EngramConfig::from_profile_file(&profile_path).expect_err("k=0 must reject");
        assert!(
            err.contains("recall_fusion"),
            "error names the section: {err}"
        );
    }

    #[test]
    fn discover_recall_fusion_absent_returns_none() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = EngramConfig::discover_recall_fusion(temp_dir.path())
            .expect("absent file is Ok(None), not Err");
        assert!(resolved.is_none(), "no .engram/recall.json => None");
    }

    #[test]
    fn discover_recall_fusion_present_loads_validated() {
        let temp_dir = TempDir::new().unwrap();
        let engram_dir = temp_dir.path().join(".engram");
        std::fs::create_dir_all(&engram_dir).unwrap();
        std::fs::write(
            engram_dir.join("recall.json"),
            r#"{"rrf_k":50,"default_source_weight":1.0,"source_weights":{"vector":0.7}}"#,
        )
        .unwrap();

        let cfg = EngramConfig::discover_recall_fusion(temp_dir.path())
            .expect("valid file loads")
            .expect("Some when present");
        assert_eq!(cfg.rrf_k, 50);
        assert_eq!(
            cfg.to_reciprocal_config().unwrap().source_weight("vector"),
            0.7
        );
    }

    #[test]
    fn discover_recall_fusion_invalid_surfaces_error() {
        let temp_dir = TempDir::new().unwrap();
        let engram_dir = temp_dir.path().join(".engram");
        std::fs::create_dir_all(&engram_dir).unwrap();
        std::fs::write(engram_dir.join("recall.json"), r#"{"rrf_k":0}"#).unwrap();

        let err = EngramConfig::discover_recall_fusion(temp_dir.path())
            .expect_err("invalid file surfaces Err");
        assert!(err.contains("recall"), "error references the file: {err}");
    }

    #[test]
    fn resolve_recall_fusion_explicit_wins_over_file() {
        // The ladder: an explicit value must win over a discovered file.
        let temp_dir = TempDir::new().unwrap();
        let engram_dir = temp_dir.path().join(".engram");
        std::fs::create_dir_all(&engram_dir).unwrap();
        std::fs::write(engram_dir.join("recall.json"), r#"{"rrf_k":50}"#).unwrap();

        let explicit = engram_retrieval::RecallFusionConfig {
            rrf_k: 99,
            default_source_weight: 1.0,
            source_weights: {
                let mut m = BTreeMap::new();
                m.insert("lexical".to_string(), 0.2);
                m
            },
            rerank: None,
        };
        let resolved = EngramConfig::resolve_recall_fusion(Some(explicit.clone()), temp_dir.path())
            .expect("ladder resolves")
            .expect("Some");
        assert_eq!(resolved.rrf_k, 99, "explicit wins over discovered file");
        assert_eq!(
            resolved
                .to_reciprocal_config()
                .unwrap()
                .source_weight("lexical"),
            0.2
        );
    }

    // ---- RFC-0019 D3 (reversed): enable_vector runtime kill-switch --------

    fn base_engram_config(temp_dir: &TempDir) -> EngramConfig {
        EngramConfig::new(
            temp_dir.path().join("engram"),
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
        )
    }

    #[test]
    fn enable_vector_defaults_to_true() {
        // The default is ON: a deployment that never touches the field keeps
        // vector wiring on when fastembed is compiled in (RFC-0019 D3 reversed).
        let temp_dir = TempDir::new().unwrap();
        let config = base_engram_config(&temp_dir);
        assert!(config.enable_vector, "enable_vector must default to true");
    }

    #[test]
    fn with_enable_vector_sets_false() {
        let temp_dir = TempDir::new().unwrap();
        let config = base_engram_config(&temp_dir).with_enable_vector(false);
        assert!(
            !config.enable_vector,
            "with_enable_vector(false) must stick"
        );
        // And back to true.
        let config = config.with_enable_vector(true);
        assert!(config.enable_vector, "with_enable_vector(true) must stick");
    }

    #[test]
    fn profile_enable_vector_section_loads() {
        // An explicit `enable_vector = false` in a profile file is honored.
        // TOML requires a bare top-level key to precede any `[section]`, so it
        // goes before `[backend]` (a key after `[backend]` would parse as
        // `backend.enable_vector`).
        let temp_dir = TempDir::new().unwrap();
        let data_root = temp_dir.path().join("data");
        std::fs::create_dir_all(&data_root).unwrap();
        let toml = format!(
            "enable_vector = false\n\n{base}",
            base = base_profile_toml(data_root.to_str().unwrap())
        );
        let profile_path = temp_dir.path().join("profile.toml");
        std::fs::write(&profile_path, &toml).unwrap();

        let cfg = EngramConfig::from_profile_file(&profile_path).expect("profile loads");
        assert!(!cfg.enable_vector, "profile enable_vector=false must load");
    }

    #[test]
    fn profile_without_enable_vector_defaults_true() {
        // A legacy profile without the field deserializes to true (backward
        // compatibility — existing configs keep vector wiring on).
        let temp_dir = TempDir::new().unwrap();
        let data_root = temp_dir.path().join("data");
        std::fs::create_dir_all(&data_root).unwrap();
        let toml = base_profile_toml(data_root.to_str().unwrap());
        let profile_path = temp_dir.path().join("profile.toml");
        std::fs::write(&profile_path, &toml).unwrap();

        let cfg = EngramConfig::from_profile_file(&profile_path).expect("profile loads");
        assert!(cfg.enable_vector, "absent field => true (default-on)");
    }

    #[test]
    fn enable_vector_round_trips_through_serde_with_default() {
        // A legacy JSON config without the field deserializes to true (serde
        // default), matching the in-memory default.
        let temp_dir = TempDir::new().unwrap();
        let config = base_engram_config(&temp_dir);
        let json = serde_json::to_string(&config).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value.as_object_mut().unwrap().remove("enable_vector");
        let legacy = serde_json::to_string(&value).unwrap();
        let parsed: EngramConfig = serde_json::from_str(&legacy).unwrap();
        assert!(parsed.enable_vector, "absent serde field => true");
    }
}
