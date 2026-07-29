//! Launch configuration parsed from argv (env variants land with the ontology
//! loader in T4).

use std::path::PathBuf;

use engram_domain::ScopeMappingStrategy;
use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, MigrationMode};

/// SQLite storage layout the server opens. `Single` (the default) writes one
/// shared database file — matching the agentzero adapter invariant so the same
/// database is consumable by the gateway. `Multi` keeps the per-store default
/// (one file per family) for tests and advanced use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpSqliteLayout {
    Single,
    Multi,
}

impl Default for McpSqliteLayout {
    fn default() -> Self {
        Self::Single
    }
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
}

impl McpConfig {
    /// Parse `--storage`, `--project`, `--ontology`, `--taxonomy` flags.
    /// `--storage` is required; `--project` defaults to `"default"`.
    pub fn from_args(argv: &[String]) -> Result<Self, String> {
        let mut storage_path = None;
        let mut project = None;
        let mut ontology_path = None;
        let mut taxonomy_path = None;
        let mut sqlite_layout: Option<McpSqliteLayout> = None;
        let mut db_file: Option<String> = None;
        let mut i = 0;
        while i < argv.len() {
            let flag = argv[i].as_str();
            if !matches!(
                flag,
                "--storage" | "--project" | "--ontology" | "--taxonomy" | "--layout" | "--db-file"
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
                provider_type: "none".to_owned(),
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
}
