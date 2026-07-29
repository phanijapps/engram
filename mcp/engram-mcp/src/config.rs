//! Launch configuration parsed from argv (env variants land with the ontology
//! loader in T4).

use std::path::PathBuf;

use engram_domain::ScopeMappingStrategy;
use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, MigrationMode};

/// Everything the server needs to open a provider and resolve scope.
///
/// `ontology_path` / `taxonomy_path` are accepted here and consumed by the
/// loader in T4; Phase 1's generic core ignores them when absent (baked-in
/// default).
#[allow(dead_code)] // project read by scope (T3); ontology/taxonomy by the loader (T4)
pub struct McpConfig {
    pub storage_path: PathBuf,
    pub project: String,
    pub scope_strategy: ScopeMappingStrategy,
    pub embedding: EmbeddingProviderConfig,
    pub migration_mode: MigrationMode,
    pub capability_policy: CapabilityPolicy,
    pub ontology_path: Option<PathBuf>,
    pub taxonomy_path: Option<PathBuf>,
}

impl McpConfig {
    /// Parse `--storage`, `--project`, `--ontology`, `--taxonomy` flags.
    /// `--storage` is required; `--project` defaults to `"default"`.
    pub fn from_args(argv: &[String]) -> Result<Self, String> {
        let mut storage_path = None;
        let mut project = None;
        let mut ontology_path = None;
        let mut taxonomy_path = None;
        let mut i = 0;
        while i < argv.len() {
            match argv[i].as_str() {
                "--storage" => {
                    storage_path = argv.get(i + 1).map(PathBuf::from);
                    i += 2;
                }
                "--project" => {
                    project = argv.get(i + 1).cloned();
                    i += 2;
                }
                "--ontology" => {
                    ontology_path = argv.get(i + 1).map(PathBuf::from);
                    i += 2;
                }
                "--taxonomy" => {
                    taxonomy_path = argv.get(i + 1).map(PathBuf::from);
                    i += 2;
                }
                other => {
                    return Err(format!("unknown argument: {other}"));
                }
            }
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
    fn from_args_rejects_unknown_flag() {
        let argv = ["--bogus".to_string()];
        assert!(McpConfig::from_args(&argv).is_err());
    }
}
