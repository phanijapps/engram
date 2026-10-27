//! Multi-layer ontology + taxonomy launch configuration.
//!
//! Supplied at server start via `--ontology <path>` / `--taxonomy <path>` (JSON
//! files); a missing config falls back to a baked-in generic default so the
//! server runs zero-config. The active config is the source of truth that
//! `ontology_read` / `taxonomy_read` return to agents and that the agent-side
//! `engram-distill` skill classifies against. Persisting it into the
//! `OntologyRepository` / `TaxonomyRepository` (for graph governance) is T4b.
//!
//! Layer model: a *layer* is a named set of classes (technical / business /
//! domain / custom); `within` predicates relate classes inside a layer,
//! `across` predicates bridge layers (e.g. `realized_by` ties a business
//! concept to a technical artifact).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol;

/// One ontology layer: a name plus its concept classes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyLayer {
    pub name: String,
    #[serde(default)]
    pub classes: Vec<String>,
}

/// Allowed predicates: `within` a layer and `across` layers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyPredicates {
    #[serde(default)]
    pub within: Vec<String>,
    #[serde(default)]
    pub across: Vec<String>,
}

/// The active multi-layer ontology configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyConfig {
    pub layers: Vec<OntologyLayer>,
    #[serde(default)]
    pub predicates: OntologyPredicates,
}

impl Default for OntologyConfig {
    /// Baked-in generic default: a 3-layer technology ontology (a rich
    /// `technical` layer + light `domain`/`business` layers) with standard
    /// `within`/`across` predicates. Lets the server run with a meaningful
    /// vocabulary when no `--ontology` file is supplied; the `across`
    /// predicates (`describes`, `realized_by`, …) are what bridge layers —
    /// e.g. a domain concept `describes` a technical artifact.
    fn default() -> Self {
        Self {
            layers: vec![
                OntologyLayer {
                    name: "technical".to_owned(),
                    classes: vec![
                        "Module".to_owned(),
                        "Package".to_owned(),
                        "Namespace".to_owned(),
                        "File".to_owned(),
                        "Function".to_owned(),
                        "Method".to_owned(),
                        "Class".to_owned(),
                        "Struct".to_owned(),
                        "Interface".to_owned(),
                        "Trait".to_owned(),
                        "Type".to_owned(),
                        "Endpoint".to_owned(),
                        "Service".to_owned(),
                        "Component".to_owned(),
                        "Configuration".to_owned(),
                        "Dependency".to_owned(),
                    ],
                },
                OntologyLayer {
                    name: "domain".to_owned(),
                    classes: vec![
                        "Concept".to_owned(),
                        "Feature".to_owned(),
                        "Requirement".to_owned(),
                        "Behavior".to_owned(),
                    ],
                },
                OntologyLayer {
                    name: "business".to_owned(),
                    classes: vec![
                        "Capability".to_owned(),
                        "Workflow".to_owned(),
                        "Stakeholder".to_owned(),
                    ],
                },
            ],
            predicates: OntologyPredicates {
                within: vec![
                    "calls".to_owned(),
                    "depends_on".to_owned(),
                    "implements".to_owned(),
                    "contains".to_owned(),
                    "part_of".to_owned(),
                    "uses".to_owned(),
                    "defines".to_owned(),
                    "references".to_owned(),
                ],
                across: vec![
                    "describes".to_owned(),
                    "realized_by".to_owned(),
                    "governs".to_owned(),
                    "extracted_from".to_owned(),
                    "distilled_from".to_owned(),
                ],
            },
        }
    }
}

impl OntologyConfig {
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("parse ontology config: {e}"))
    }
}

/// One taxonomy concept: a label and an optional broader concept label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyConcept {
    pub label: String,
    #[serde(default)]
    pub broader: Option<String>,
}

/// The active taxonomy configuration: one concept scheme + its concepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyConfig {
    pub name: String,
    #[serde(default)]
    pub concepts: Vec<TaxonomyConcept>,
}

impl Default for TaxonomyConfig {
    /// Baked-in generic default: a small SKOS-aligned concept scheme so recall
    /// has a non-trivial hierarchy when no `--taxonomy` file is supplied.
    /// `label` maps to `skos:prefLabel`, `broader` to `skos:broader`.
    fn default() -> Self {
        Self {
            name: "software".to_owned(),
            concepts: vec![
                TaxonomyConcept {
                    label: "Software".to_owned(),
                    broader: None,
                },
                TaxonomyConcept {
                    label: "Code".to_owned(),
                    broader: Some("Software".to_owned()),
                },
                TaxonomyConcept {
                    label: "Documentation".to_owned(),
                    broader: Some("Software".to_owned()),
                },
                TaxonomyConcept {
                    label: "Configuration".to_owned(),
                    broader: Some("Software".to_owned()),
                },
                TaxonomyConcept {
                    label: "Test".to_owned(),
                    broader: Some("Code".to_owned()),
                },
            ],
        }
    }
}

impl TaxonomyConfig {
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("parse taxonomy config: {e}"))
    }
}

/// Resolve the ontology config from a file path, or fall back to the default.
pub fn resolve_ontology_config(path: Option<&Path>) -> Result<OntologyConfig, String> {
    match path {
        Some(p) => {
            let text = std::fs::read_to_string(p)
                .map_err(|e| format!("read ontology config {}: {e}", p.display()))?;
            OntologyConfig::from_json(&text)
        }
        None => Ok(OntologyConfig::default()),
    }
}

/// The conventional per-workspace launch path (DEC-P03):
/// `<storage>/packs/launch/<workspace>/ontology.json`.
pub fn conventional_ontology_path(storage: &Path, workspace: &str) -> PathBuf {
    storage
        .join("packs")
        .join("launch")
        .join(workspace)
        .join("ontology.json")
}

/// The conventional per-workspace taxonomy path (DEC-P03).
pub fn conventional_taxonomy_path(storage: &Path, workspace: &str) -> PathBuf {
    storage
        .join("packs")
        .join("launch")
        .join(workspace)
        .join("taxonomy.json")
}

/// Fallback-only conventional resolution (DEC-P03): the conventional file
/// loads ONLY when no explicit flag is passed AND the file exists; the
/// explicit flag always wins; with neither, the baked-in default returns
/// unchanged (no-pack behavior stays byte-identical, AC-011).
/// A malformed conventional file is an honest error — it is never silently
/// skipped, because a pack install wrote it and hiding it would hide the
/// pack.
pub fn resolve_ontology_config_with_conventional(
    flag: Option<&Path>,
    conventional: Option<&Path>,
) -> Result<OntologyConfig, String> {
    match (flag, conventional.filter(|p| p.exists())) {
        (Some(explicit), _) => resolve_ontology_config(Some(explicit)),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path).map_err(|e| {
                format!("read conventional ontology config {}: {e}", path.display())
            })?;
            OntologyConfig::from_json(&text).map_err(|e| {
                format!(
                    "conventional launch config {} is malformed — delete it or re-run \
                     `engram-mcp pack install`: {e}",
                    path.display()
                )
            })
        }
        (None, None) => Ok(OntologyConfig::default()),
    }
}

/// Taxonomy twin of [`resolve_ontology_config_with_conventional`].
pub fn resolve_taxonomy_config_with_conventional(
    flag: Option<&Path>,
    conventional: Option<&Path>,
) -> Result<TaxonomyConfig, String> {
    match (flag, conventional.filter(|p| p.exists())) {
        (Some(explicit), _) => resolve_taxonomy_config(Some(explicit)),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path).map_err(|e| {
                format!("read conventional taxonomy config {}: {e}", path.display())
            })?;
            TaxonomyConfig::from_json(&text).map_err(|e| {
                format!(
                    "conventional taxonomy config {} is malformed — delete it or re-run \
                     `engram-mcp pack install`: {e}",
                    path.display()
                )
            })
        }
        (None, None) => Ok(TaxonomyConfig::default()),
    }
}

/// Resolve the taxonomy config from a file path, or fall back to the default.
pub fn resolve_taxonomy_config(path: Option<&Path>) -> Result<TaxonomyConfig, String> {
    match path {
        Some(p) => {
            let text = std::fs::read_to_string(p)
                .map_err(|e| format!("read taxonomy config {}: {e}", p.display()))?;
            TaxonomyConfig::from_json(&text)
        }
        None => Ok(TaxonomyConfig::default()),
    }
}

/// Render the active ontology config as an MCP text result.
pub fn ontology_config_as_text(cfg: &OntologyConfig) -> Value {
    match serde_json::to_string_pretty(cfg) {
        Ok(text) => protocol::text_content(text),
        Err(e) => protocol::text_content(format!("error serializing ontology config: {e}")),
    }
}

/// Render the active taxonomy config as an MCP text result.
pub fn taxonomy_config_as_text(cfg: &TaxonomyConfig) -> Value {
    match serde_json::to_string_pretty(cfg) {
        Ok(text) => protocol::text_content(text),
        Err(e) => protocol::text_content(format!("error serializing taxonomy config: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_multilayer_ontology() {
        let cfg = OntologyConfig::from_json(
            r#"{
                "layers": [
                    {"name": "technical", "classes": ["Service", "Api"]},
                    {"name": "business", "classes": ["Customer", "Order"]}
                ],
                "predicates": {"within": ["depends_on"], "across": ["realized_by"]}
            }"#,
        )
        .unwrap();
        assert_eq!(cfg.layers.len(), 2);
        assert_eq!(cfg.layers[0].name, "technical");
        assert_eq!(cfg.layers[0].classes, vec!["Service", "Api"]);
        assert_eq!(cfg.predicates.within, vec!["depends_on"]);
        assert_eq!(cfg.predicates.across, vec!["realized_by"]);
    }

    #[test]
    fn default_ontology_is_generic_technology() {
        let cfg = OntologyConfig::default();
        assert!(
            cfg.layers.len() >= 3,
            "default should be multi-layer (technical/domain/business)"
        );
        assert!(
            cfg.layers.iter().any(|l| l.name == "technical"),
            "default should include a technical layer"
        );
        assert!(!cfg.predicates.within.is_empty());
        assert!(!cfg.predicates.across.is_empty());
        assert!(
            cfg.predicates.across.iter().any(|p| p == "describes"),
            "default across predicates should include describes"
        );
    }

    #[test]
    fn default_taxonomy_is_skos_tree() {
        let cfg = TaxonomyConfig::default();
        assert!(
            cfg.concepts.len() > 1,
            "default taxonomy should be a tree, not a single concept"
        );
        assert!(
            cfg.concepts.iter().any(|c| c.broader.is_some()),
            "default taxonomy should have at least one broader (skos:broader) link"
        );
    }

    #[test]
    fn parse_taxonomy_with_broader() {
        let cfg = TaxonomyConfig::from_json(
            r#"{"name":"ops","concepts":[{"label":"Root"},{"label":"Child","broader":"Root"}]}"#,
        )
        .unwrap();
        assert_eq!(cfg.name, "ops");
        assert_eq!(cfg.concepts.len(), 2);
        assert_eq!(cfg.concepts[1].broader.as_deref(), Some("Root"));
    }

    #[test]
    fn resolve_ontology_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("o.json");
        std::fs::write(
            &path,
            r#"{"layers":[{"name":"technical","classes":["Service"]}],"predicates":{}}"#,
        )
        .unwrap();
        let cfg = resolve_ontology_config(Some(&path)).unwrap();
        assert_eq!(cfg.layers[0].name, "technical");
    }

    #[test]
    fn resolve_ontology_defaults_when_no_path() {
        let cfg = resolve_ontology_config(None).unwrap();
        assert_eq!(cfg, OntologyConfig::default());
    }

    #[test]
    fn resolve_explicit_flag_wins_over_conventional() {
        let dir = tempfile::tempdir().unwrap();
        let flag_path = dir.path().join("flag.json");
        std::fs::write(
            &flag_path,
            r#"{"layers":[{"name":"technical","classes":["FromFlag"]}],"predicates":{}}"#,
        )
        .unwrap();
        let conventional = dir.path().join("conventional.json");
        std::fs::write(
            &conventional,
            r#"{"layers":[{"name":"technical","classes":["FromPack"]}],"predicates":{}}"#,
        )
        .unwrap();
        let cfg = resolve_ontology_config_with_conventional(Some(&flag_path), Some(&conventional))
            .unwrap();
        assert_eq!(cfg.layers[0].classes, vec!["FromFlag".to_owned()]);
    }

    #[test]
    fn resolve_conventional_loads_only_when_no_flag_and_present() {
        let dir = tempfile::tempdir().unwrap();
        let conventional = dir.path().join("conventional.json");
        std::fs::write(
            &conventional,
            r#"{"layers":[{"name":"technical","classes":["FromPack"]}],"predicates":{}}"#,
        )
        .unwrap();
        let cfg = resolve_ontology_config_with_conventional(None, Some(&conventional)).unwrap();
        assert_eq!(cfg.layers[0].classes, vec!["FromPack".to_owned()]);
        // The conventional path may also be absent.
        let absent = dir.path().join("missing.json");
        let cfg = resolve_ontology_config_with_conventional(None, Some(&absent)).unwrap();
        assert_eq!(cfg, OntologyConfig::default());
    }

    #[test]
    fn resolve_absent_conventional_is_the_byte_identical_default() {
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("missing.json");
        let cfg = resolve_ontology_config_with_conventional(None, Some(&absent)).unwrap();
        assert_eq!(cfg, OntologyConfig::default());
        let taxonomy = resolve_taxonomy_config_with_conventional(None, Some(&absent)).unwrap();
        assert_eq!(taxonomy, TaxonomyConfig::default());
    }

    #[test]
    fn resolve_malformed_conventional_is_an_honest_error() {
        let dir = tempfile::tempdir().unwrap();
        let conventional = dir.path().join("conventional.json");
        std::fs::write(&conventional, "{ not json").unwrap();
        let err = resolve_ontology_config_with_conventional(None, Some(&conventional)).unwrap_err();
        assert!(err.contains("malformed"), "got: {err}");
        assert!(err.contains("pack install"), "names the repair: {err}");
    }

    #[test]
    fn resolve_conventional_paths_are_per_workspace() {
        let storage = std::path::Path::new("/tmp/somewhere");
        assert_eq!(
            conventional_ontology_path(storage, "proj-a"),
            std::path::PathBuf::from("/tmp/somewhere/packs/launch/proj-a/ontology.json")
        );
        assert_eq!(
            conventional_taxonomy_path(storage, "proj-b"),
            std::path::PathBuf::from("/tmp/somewhere/packs/launch/proj-b/taxonomy.json")
        );
    }

    #[test]
    fn ontology_config_as_text_contains_layer_name() {
        let text = ontology_config_as_text(&OntologyConfig::default());
        let body = text["content"][0]["text"].as_str().unwrap();
        assert!(body.contains("technical"), "got: {body}");
    }
}
