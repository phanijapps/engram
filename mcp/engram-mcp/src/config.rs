//! Launch configuration parsed from argv (env variants land with the ontology
//! loader in T4).

use std::path::PathBuf;

use engram_domain::ScopeMappingStrategy;
use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, MigrationMode};

/// SQLite storage layout the server opens. `Single` (the default) writes one
/// shared database file — matching the agentzero adapter invariant so the same
/// database is consumable by the gateway. `Multi` keeps the per-store default
/// (one file per family) for tests and advanced use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpSqliteLayout {
    #[default]
    Single,
    Multi,
}

/// Everything the server needs to open a provider and resolve scope.
///
/// `ontology_path` / `taxonomy_path` are accepted here and consumed by the
/// loader; the generic core ignores them when absent (baked-in default).
#[derive(Debug)]
pub struct McpConfig {
    pub storage_path: PathBuf,
    pub project: String,
    pub scope_strategy: ScopeMappingStrategy,
    pub embedding: EmbeddingProviderConfig,
    pub migration_mode: MigrationMode,
    pub capability_policy: CapabilityPolicy,
    pub ontology_path: Option<PathBuf>,
    pub taxonomy_path: Option<PathBuf>,
    pub sqlite_layout: McpSqliteLayout,
    pub db_file: String,
    pub org: Option<String>,
    pub domain: Option<String>,
    pub subdomain: Option<String>,
    /// Runtime kill-switch for the vector lane (RFC-0019 D3 reversed). Defaults
    /// to `true`. `--no-vector` sets it `false`, which causes the bootstrap to
    /// skip the vector index + embedding provider/model + vector recall lane —
    /// even when fastembed is compiled in — so a deployment can avoid the model
    /// download/load without a rebuild.
    pub enable_vector: bool,
    /// Tool profile: restricts which MCP tools are registered. Empty = all tools.
    /// Values: "investigate", "read", "scan", "write", "maintain", or empty (all).
    pub tool_profile: String,
}

impl McpConfig {
    /// Parse `--storage`, `--project`, `--ontology`, `--taxonomy` flags.
    /// `--storage` is required; `--project` defaults to `"default"`.
    /// `--no-vector` / `--vector` are value-less switches (RFC-0019 D3
    /// reversed) toggling `enable_vector` (default `true`).
    pub fn from_args(argv: &[String]) -> Result<Self, String> {
        let mut storage_path = None;
        let mut project = None;
        let mut ontology_path = None;
        let mut taxonomy_path = None;
        let mut sqlite_layout: Option<McpSqliteLayout> = None;
        let mut db_file: Option<String> = None;
        let mut org: Option<String> = None;
        let mut domain: Option<String> = None;
        let mut subdomain: Option<String> = None;
        let mut tool_profile = String::new();
        // Default-on (RFC-0019 D3 reversed): vector compiles in with the
        // `fastembed` default feature; `--no-vector` disables it at runtime
        // without a rebuild.
        let mut enable_vector = true;
        let mut i = 0;
        while i < argv.len() {
            let flag = argv[i].as_str();
            // Value-less switches: handle before the value-consuming path.
            match flag {
                "--no-vector" => {
                    enable_vector = false;
                    i += 1;
                    continue;
                }
                "--vector" => {
                    enable_vector = true;
                    i += 1;
                    continue;
                }
                _ => {}
            }
            if !matches!(
                flag,
                "--storage"
                    | "--project"
                    | "--ontology"
                    | "--taxonomy"
                    | "--layout"
                    | "--db-file"
                    | "--org"
                    | "--domain"
                    | "--subdomain"
                    | "--tools"
            ) {
                return Err(format!("unknown argument: {flag}"));
            }
            // A known flag must be followed by a value that is not itself a flag.
            let value = argv
                .get(i + 1)
                .filter(|v| !v.starts_with("--"))
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value"))?;
            match flag {
                "--storage" => storage_path = Some(PathBuf::from(value)),
                "--project" => project = Some(value),
                "--ontology" => ontology_path = Some(PathBuf::from(value)),
                "--taxonomy" => taxonomy_path = Some(PathBuf::from(value)),
                "--layout" => {
                    sqlite_layout = Some(match value.as_str() {
                        "single" => McpSqliteLayout::Single,
                        "multi" => McpSqliteLayout::Multi,
                        other => {
                            return Err(format!(
                                "unknown --layout value: {other}; expected single|multi"
                            ));
                        }
                    });
                }
                "--db-file" => db_file = Some(value),
                "--org" => org = Some(value),
                "--domain" => domain = Some(value),
                "--subdomain" => subdomain = Some(value),
                "--tools" => tool_profile = value,
                _ => unreachable!("known flags are matched above"),
            }
            i += 2;
        }
        let storage_path = storage_path.ok_or("missing required --storage <path>")?;
        Ok(Self {
            storage_path,
            project: project.unwrap_or_else(|| "default".to_string()),
            scope_strategy: ScopeMappingStrategy::Strict,
            embedding: EmbeddingProviderConfig {
                #[cfg(feature = "fastembed")]
                provider_type: "fastembed".to_owned(),
                #[cfg(not(feature = "fastembed"))]
                provider_type: "none".to_owned(),
                #[cfg(feature = "fastembed")]
                model: "BAAI/bge-small-en-v1.5".to_owned(),
                #[cfg(not(feature = "fastembed"))]
                model: "none".to_owned(),
                dimensions: 384,
                prompt_profile: "query".to_owned(),
                normalization: None,
            },
            migration_mode: MigrationMode::DryRun,
            capability_policy: CapabilityPolicy::FailClosed,
            ontology_path,
            taxonomy_path,
            sqlite_layout: sqlite_layout.unwrap_or_default(),
            db_file: db_file.unwrap_or_else(|| "engram_data.db".to_string()),
            org,
            domain,
            subdomain,
            enable_vector,
            tool_profile,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_args_parses_flags() {
        let argv = [
            "--storage".to_string(),
            "/tmp/x".to_string(),
            "--project".to_string(),
            "myproj".to_string(),
            "--ontology".to_string(),
            "/tmp/o.toml".to_string(),
        ];
        let c = McpConfig::from_args(&argv).unwrap();
        assert_eq!(c.storage_path, PathBuf::from("/tmp/x"));
        assert_eq!(c.project, "myproj");
        assert_eq!(
            c.ontology_path.as_deref(),
            Some(std::path::Path::new("/tmp/o.toml"))
        );
        assert_eq!(c.taxonomy_path, None);
        // `provider_type` is feature-dependent: fastembed (now the default)
        // selects "fastembed"; a `--no-default-features` build selects "none".
        #[cfg(feature = "fastembed")]
        assert_eq!(c.embedding.provider_type, "fastembed");
        #[cfg(not(feature = "fastembed"))]
        assert_eq!(c.embedding.provider_type, "none");
    }

    #[test]
    fn from_args_requires_storage() {
        assert!(McpConfig::from_args(&[]).is_err());
    }

    #[test]
    fn from_args_defaults_project() {
        let argv = ["--storage".to_string(), "/tmp/x".to_string()];
        let c = McpConfig::from_args(&argv).unwrap();
        assert_eq!(c.project, "default");
    }

    #[test]
    fn from_args_defaults_to_single_file_layout() {
        let argv = ["--storage".to_string(), "/tmp/x".to_string()];
        let c = McpConfig::from_args(&argv).unwrap();
        assert_eq!(c.sqlite_layout, McpSqliteLayout::Single);
        assert_eq!(c.db_file, "engram_data.db");
    }

    #[test]
    fn from_args_parses_layout_and_db_file() {
        let argv = [
            "--storage".to_string(),
            "/tmp/x".to_string(),
            "--layout".to_string(),
            "multi".to_string(),
            "--db-file".to_string(),
            "custom.sqlite".to_string(),
        ];
        let c = McpConfig::from_args(&argv).unwrap();
        assert_eq!(c.sqlite_layout, McpSqliteLayout::Multi);
        assert_eq!(c.db_file, "custom.sqlite");
    }

    #[test]
    fn from_args_rejects_unknown_layout() {
        let argv = [
            "--storage".to_string(),
            "/tmp/x".to_string(),
            "--layout".to_string(),
            "sharded".to_string(),
        ];
        let err = McpConfig::from_args(&argv).unwrap_err();
        assert!(err.contains("expected single|multi"), "got: {err}");
    }

    #[test]
    fn from_args_rejects_unknown_flag() {
        let argv = ["--bogus".to_string()];
        assert!(McpConfig::from_args(&argv).is_err());
    }

    #[test]
    fn from_args_flag_requires_a_value() {
        let argv = ["--storage".to_string()];
        let err = McpConfig::from_args(&argv).unwrap_err();
        assert!(err.contains("requires a value"), "got: {err}");
    }

    #[test]
    fn from_args_rejects_value_that_looks_like_a_flag() {
        let argv = ["--storage".to_string(), "--project".to_string()];
        let err = McpConfig::from_args(&argv).unwrap_err();
        assert!(err.contains("requires a value"), "got: {err}");
    }

    // ---- RFC-0019 D3 (reversed): --no-vector / --vector runtime switch -----

    #[test]
    fn from_args_defaults_enable_vector_true() {
        let argv = ["--storage".to_string(), "/tmp/x".to_string()];
        let c = McpConfig::from_args(&argv).unwrap();
        assert!(c.enable_vector, "enable_vector defaults to true");
    }

    #[test]
    fn from_args_no_vector_disables_vector() {
        let argv = [
            "--storage".to_string(),
            "/tmp/x".to_string(),
            "--no-vector".to_string(),
        ];
        let c = McpConfig::from_args(&argv).unwrap();
        assert!(!c.enable_vector, "--no-vector must set enable_vector=false");
    }

    #[test]
    fn from_args_vector_re_enables_after_no_vector() {
        // `--vector` re-enables; last-one-wins lets operators toggle in scripts.
        let argv = [
            "--storage".to_string(),
            "/tmp/x".to_string(),
            "--no-vector".to_string(),
            "--vector".to_string(),
        ];
        let c = McpConfig::from_args(&argv).unwrap();
        assert!(c.enable_vector, "--vector must re-enable vector");
    }

    #[test]
    fn from_args_no_vector_does_not_consume_next_flag() {
        // `--no-vector` is a value-less switch: the following `--project` must
        // still parse (not be swallowed as --no-vector's value).
        let argv = [
            "--storage".to_string(),
            "/tmp/x".to_string(),
            "--no-vector".to_string(),
            "--project".to_string(),
            "p".to_string(),
        ];
        let c = McpConfig::from_args(&argv).unwrap();
        assert!(!c.enable_vector);
        assert_eq!(c.project, "p");
    }
}
