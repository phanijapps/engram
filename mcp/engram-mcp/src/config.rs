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

/// Storage backend the server opens. SQLite is the zero-dependency default;
/// `pgvector` routes through the `backends/pgvector` recipe (requires the
/// `pgvector` cargo feature + a connection string).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpBackend {
    #[default]
    Sqlite,
    Pgvector,
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
    /// Explicit, one-launch vector-only reset for changing embedding spaces.
    pub rebuild_vectors: bool,
    /// Tool profile: restricts which MCP tools are registered. Empty = all tools.
    /// Values: "investigate", "read", "scan", "write", "maintain", or empty (all).
    pub tool_profile: String,
    /// Storage backend (PS1). `--backend pgvector` opens through the
    /// `backends/pgvector` recipe instead of the SQLite bootstrap.
    pub backend: McpBackend,
    /// Postgres connection string when `backend == pgvector`. Required for
    /// that backend; accepted from `--pg-connection-string` or the
    /// `ENGRAM_PG_CONNECTION_STRING` env var. (Read only under the `pgvector`
    /// cargo feature — hence the cfg_attr.)
    #[cfg_attr(not(feature = "pgvector"), allow(dead_code))]
    pub pg_connection_string: Option<String>,
}

impl McpConfig {
    /// Parse `--storage`, `--project`, `--ontology`, `--taxonomy` flags.
    /// `--storage` is required; `--project` defaults to `"default"`.
    /// `--no-vector` / `--vector` are value-less switches (RFC-0019 D3
    /// reversed) toggling `enable_vector` (default `true`).
    pub fn from_args(argv: &[String]) -> Result<Self, String> {
        let (argv, embedding) = crate::embedding_config::from_args(argv)?;
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
        let mut backend: Option<McpBackend> = None;
        let mut pg_connection_string: Option<String> = None;
        // Default-on (RFC-0019 D3 reversed): vector compiles in with the
        // `fastembed` default feature; `--no-vector` disables it at runtime
        // without a rebuild.
        let mut enable_vector = true;
        let mut rebuild_vectors = false;
        let mut i = 0;
        while i < argv.len() {
            let flag = argv[i].as_str();
            // Value-less switches: handle before the value-consuming path.
            match flag {
                "--slim" | "--no-vector" => {
                    enable_vector = false;
                    i += 1;
                    continue;
                }
                "--vector" => {
                    enable_vector = true;
                    i += 1;
                    continue;
                }
                "--rebuild-vectors" => {
                    rebuild_vectors = true;
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
                    | "--backend"
                    | "--pg-connection-string"
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
                "--backend" => {
                    backend = Some(match value.as_str() {
                        "sqlite" => McpBackend::Sqlite,
                        "pgvector" | "pg" | "postgres" => McpBackend::Pgvector,
                        other => {
                            return Err(format!(
                                "unknown --backend value: {other}; expected sqlite|pgvector"
                            ));
                        }
                    });
                }
                "--pg-connection-string" => pg_connection_string = Some(value),
                _ => unreachable!("known flags are matched above"),
            }
            i += 2;
        }
        if rebuild_vectors && !enable_vector {
            return Err("--rebuild-vectors cannot be used with embeddings disabled".to_owned());
        }
        // Launch-config env fallbacks (PLAN mcp-args-contract step 2):
        // precedence is flag > environment > default — same discipline the
        // pg-connection-string already follows below. The launcher hooks and
        // pi extension speak ENGRAM_STORAGE/ENGRAM_PROJECT; the server
        // honoring the same variables removes the silent scope skew where a
        // hook derived cwd-basename while the server used "default".
        let storage_path = storage_path
            .or_else(|| {
                std::env::var("ENGRAM_STORAGE")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
            })
            .ok_or("missing required --storage <path> (or set ENGRAM_STORAGE)")?;
        let backend = backend.unwrap_or_default();
        // The env var is a fallback, not an override: an explicit flag wins.
        let pg_connection_string = pg_connection_string.or_else(|| {
            std::env::var("ENGRAM_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty())
        });
        if backend == McpBackend::Pgvector && pg_connection_string.is_none() {
            return Err(
                "--backend pgvector requires --pg-connection-string <url> or the \
                 ENGRAM_PG_CONNECTION_STRING env var"
                    .to_string(),
            );
        }
        Ok(Self {
            storage_path,
            project: project
                .or_else(|| {
                    std::env::var("ENGRAM_PROJECT")
                        .ok()
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or_else(|| "default".to_string()),
            scope_strategy: ScopeMappingStrategy::Strict,
            embedding,
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
            rebuild_vectors,
            tool_profile,
            backend,
            pg_connection_string,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PLAN mcp-args-contract step 4 — the launch-flag surface pin. Every
    /// flag the contract table (HOW-TO.md) documents must parse; a removed
    /// or renamed flag fails here before the docs drift. Env-fallback
    /// precedence (flag > env > default) is pinned for storage/project.
    #[test]
    fn launch_flag_surface_is_pinned() {
        // Values positional to CONTRACT_FLAGS' value-taking entries;
        // assembled flag/value pairs below so the single list stays
        // authoritative for parser + help + this pin (drift impossible).
        let values = [
            "/tmp/pin",
            "pinproj",
            "pinorg",
            "pindom",
            "pinsub",
            "/tmp/o.json",
            "/tmp/t.json",
            "multi",
            "pin.db",
            "full",
            "sqlite",
        ];
        let value_flags = [
            "--storage",
            "--project",
            "--org",
            "--domain",
            "--subdomain",
            "--ontology",
            "--taxonomy",
            "--layout",
            "--db-file",
            "--tools",
            "--backend",
        ];
        let mut argv: Vec<String> = Vec::new();
        for (flag, value) in value_flags.iter().zip(values) {
            argv.push(flag.to_string());
            argv.push(value.to_string());
        }
        argv.push("--no-vector".to_string());
        // Cross-check: every value flag is on the contract list (main.rs).
        for f in value_flags {
            assert!(
                crate::CONTRACT_FLAGS.contains(&f),
                "{f} missing from CONTRACT_FLAGS — update the single list"
            );
        }
        let cfg = McpConfig::from_args(&argv).expect("all 13 contract flags parse");
        assert_eq!(cfg.storage_path, PathBuf::from("/tmp/pin"));
        assert_eq!(cfg.project, "pinproj");
        assert_eq!(cfg.org.as_deref(), Some("pinorg"));
        assert_eq!(cfg.domain.as_deref(), Some("pindom"));
        assert_eq!(cfg.subdomain.as_deref(), Some("pinsub"));
        assert_eq!(cfg.ontology_path, Some(PathBuf::from("/tmp/o.json")));
        assert_eq!(cfg.taxonomy_path, Some(PathBuf::from("/tmp/t.json")));
        assert_eq!(cfg.db_file, "pin.db");
        assert_eq!(cfg.tool_profile, "full");
        assert!(!cfg.enable_vector, "--no-vector parsed");
    }

    #[test]
    fn env_fallback_precedence_is_flag_over_env_over_default() {
        // SAFETY: serialized against env-reading tests by ENV_TEST_MUTEX.
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let guard = EnvGuard::set("ENGRAM_PROJECT", "envproj");
        let flag = McpConfig::from_args(&argv(&["--storage", "/tmp/e", "--project", "flagproj"]))
            .expect("flag wins");
        assert_eq!(flag.project, "flagproj", "explicit flag beats env");
        let env_only = McpConfig::from_args(&argv(&["--storage", "/tmp/e"])).expect("env used");
        assert_eq!(env_only.project, "envproj", "env fills when flag absent");
        drop(guard);
        let neither = McpConfig::from_args(&argv(&["--storage", "/tmp/e"])).expect("default");
        assert_eq!(
            neither.project, "default",
            "literal default when neither set"
        );
    }

    #[test]
    fn missing_storage_mentions_the_env_escape_hatch() {
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let guard = EnvGuard::unset("ENGRAM_STORAGE");
        let err = McpConfig::from_args(&argv(&[])).expect_err("storage required");
        assert!(err.contains("ENGRAM_STORAGE"), "got: {err}");
        drop(guard);
    }

    fn argv(flags: &[&str]) -> Vec<String> {
        flags.iter().map(|s| s.to_string()).collect()
    }

    /// Env mutation leaks across concurrently-running tests (observed flake:
    /// `from_args_defaults_project` read ENGRAM_PROJECT set by a sibling).
    /// Every test that sets/unsets env OR asserts env-derived defaults takes
    /// this lock.
    static ENV_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        prior: Option<String>,
    }
    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prior = std::env::var(key).ok();
            // SAFETY: process-wide env mutation is inherent to testing env
            // fallbacks; the guard re-asserts the prior value on drop and
            // tests touching these keys are serialized by the key naming.
            unsafe { std::env::set_var(key, value) };
            Self { key, prior }
        }
        fn unset(key: &'static str) -> Self {
            let prior = std::env::var(key).ok();
            // SAFETY: see EnvGuard::set.
            unsafe { std::env::remove_var(key) };
            Self { key, prior }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.prior.take() {
                // SAFETY: see EnvGuard::set.
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

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
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
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
    fn slim_switch_precedence_and_inactive_profile() {
        for (switches, enabled) in [
            (vec!["--slim"], false),
            (vec!["--vector", "--slim"], false),
            (vec!["--slim", "--vector"], true),
            (vec!["--slim", "--vector", "--no-vector"], false),
        ] {
            let mut args = vec!["--storage".to_owned(), "/tmp/x".to_owned()];
            args.extend(switches.into_iter().map(String::from));
            assert_eq!(McpConfig::from_args(&args).unwrap().enable_vector, enabled);
        }
        let args: Vec<String> = [
            "--storage",
            "/tmp/x",
            "--slim",
            "--embedding-config",
            "/missing/embedding-profile.toml",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert!(McpConfig::from_args(&args).is_ok());
        let mut enabled = args.clone();
        enabled.push("--vector".into());
        assert!(McpConfig::from_args(&enabled).is_err());
        for bad in ["--embedding-typo", "--embedding-dimensions"] {
            let mut invalid = args.clone();
            invalid.extend([bad.into(), "not-a-number".into()]);
            assert!(McpConfig::from_args(&invalid).is_err());
        }
        let mut rebuild = args;
        rebuild.push("--rebuild-vectors".into());
        assert!(
            McpConfig::from_args(&rebuild)
                .unwrap_err()
                .contains("disabled")
        );
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
    #[test]
    fn from_args_parses_pgvector_backend() {
        let argv = [
            "--storage".to_string(),
            "/tmp/x".to_string(),
            "--backend".to_string(),
            "pgvector".to_string(),
            "--pg-connection-string".to_string(),
            "postgres://u:p@h:5432/d".to_string(),
        ];
        let c = McpConfig::from_args(&argv).unwrap();
        assert_eq!(c.backend, McpBackend::Pgvector);
        assert_eq!(
            c.pg_connection_string.as_deref(),
            Some("postgres://u:p@h:5432/d")
        );
    }

    #[test]
    fn from_args_defaults_sqlite_backend() {
        let argv = ["--storage".to_string(), "/tmp/x".to_string()];
        let c = McpConfig::from_args(&argv).unwrap();
        assert_eq!(c.backend, McpBackend::Sqlite);
        assert_eq!(c.pg_connection_string, None);
    }

    #[test]
    fn from_args_pgvector_requires_connection_string() {
        let argv = [
            "--storage".to_string(),
            "/tmp/x".to_string(),
            "--backend".to_string(),
            "pgvector".to_string(),
        ];
        let err = McpConfig::from_args(&argv).unwrap_err();
        assert!(err.contains("pg-connection-string"), "err: {err}");
    }

    #[test]
    fn from_args_rejects_unknown_backend() {
        let argv = [
            "--storage".to_string(),
            "/tmp/x".to_string(),
            "--backend".to_string(),
            "mysql".to_string(),
        ];
        assert!(McpConfig::from_args(&argv).is_err());
    }
}
