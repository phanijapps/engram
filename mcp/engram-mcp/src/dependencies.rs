//! Dependency-boundary extraction (post-index scan).
//!
//! `scan_dependencies` walks a repository AFTER `scan_repo` and extracts
//! package/crate-level dependencies from `Cargo.toml` (workspace members) and
//! `package.json` files. It creates `Module` entities for each package and a
//! `depends_on` edge from each consumer to each dependency, turning the graph
//! into a multi-package / multi-repo dependency view. Internal workspace
//! (`path =`) deps link to the member's own Module; external deps create a new
//! Module. Mirrors the `scan_protocols` post-index pattern; routes every write
//! through the provider's `require_knowledge()` handle.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::Utc;
use engram_domain::{EntityKind, EntityRef, Id, KnowledgeEntity, KnowledgeRelationship};
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::{internal, provenance, req_str};

/// `scan_dependencies`: post-index scan that extracts package/crate
/// dependencies from Cargo.toml + package.json into Module entities +
/// `depends_on` edges.
pub fn scan_dependencies(app: &App, args: &Value) -> Result<Value, ToolError> {
    let root = req_str(args, "path")?;
    let knowledge = app.provider.require_knowledge().map_err(internal)?;
    let scope = app.scope.clone();
    let now = Utc::now();
    let prov = provenance("mcp-scan-dependencies");

    let manifests = walk_manifests(Path::new(root));
    if manifests.is_empty() {
        return Ok(protocol::text_content(
            "No Cargo.toml or package.json manifests found.",
        ));
    }

    // consumer package name → set of dependency package names.
    let mut graph: HashMap<String, HashSet<String>> = HashMap::new();
    let mut files_scanned = 0usize;

    for (rel, text) in &manifests {
        files_scanned += 1;
        if rel.ends_with("Cargo.toml") {
            let (name, deps) = parse_cargo_deps(text);
            if let Some(name) = name {
                graph.entry(name).or_default().extend(deps);
            }
        } else if rel.ends_with("package.json") {
            if let Some((name, deps)) = parse_package_json_deps(text) {
                graph.entry(name).or_default().extend(deps);
            }
        }
    }

    // Collect every package name (consumers + deps) for entity creation.
    let mut all_packages: HashSet<String> = HashSet::new();
    for (consumer, deps) in &graph {
        all_packages.insert(consumer.clone());
        for d in deps {
            all_packages.insert(d.clone());
        }
    }

    // Persist Module entities.
    let mut entity_count = 0usize;
    for name in &all_packages {
        let entity = KnowledgeEntity {
            id: Id::from(format!("mod-{name}")),
            graph_id: None,
            kind: EntityKind::Module,
            name: name.clone(),
            aliases: Vec::new(),
            scope: scope.clone(),
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: prov.clone(),
            created_at: now,
            updated_at: None,
            valid_from: None,
            valid_until: None,
            metadata: None,
        };
        block_on(knowledge.put_entity(entity)).map_err(internal)?;
        entity_count += 1;
    }

    // Persist depends_on edges.
    let mut edge_count = 0usize;
    for (consumer, deps) in &graph {
        for dep in deps {
            let rel = KnowledgeRelationship {
                id: Id::from(format!("{consumer}\u{1f}depends_on\u{1f}{dep}")),
                graph_id: None,
                subject: EntityRef {
                    id: Some(Id::from(format!("mod-{consumer}"))),
                    kind: Some("module".to_owned()),
                    name: Some(consumer.clone()),
                    aliases: Vec::new(),
                },
                predicate: "depends_on".to_owned(),
                object: EntityRef {
                    id: Some(Id::from(format!("mod-{dep}"))),
                    kind: Some("module".to_owned()),
                    name: Some(dep.clone()),
                    aliases: Vec::new(),
                },
                scope: scope.clone(),
                evidence: Vec::new(),
                confidence: Some(0.9),
                provenance: prov.clone(),
                created_at: now,
                updated_at: None,
            };
            block_on(knowledge.put_relationship(rel)).map_err(internal)?;
            edge_count += 1;
        }
    }

    Ok(protocol::text_content(format!(
        "Dependency scan: {files_scanned} manifests, {entity_count} package entities, \
         {edge_count} depends_on edges. Use graph_neighbors on a package name to traverse.",
    )))
}

// --- pure parsers ---

/// Parse a Cargo.toml into `(package name, dependency names)`.
///
/// Tracks the current table (`[package]` captures `name`; `[dependencies]`,
/// `[dev-dependencies]`, `[build-dependencies]` capture each key as a dep). A
/// virtual workspace manifest (no `[package]`) yields `(None, deps)` — callers
/// skip it since it has no consumer identity. Line-based; no `toml` dependency.
pub fn parse_cargo_deps(toml: &str) -> (Option<String>, Vec<String>) {
    let mut name: Option<String> = None;
    let mut deps: Vec<String> = Vec::new();
    let mut section = String::new();
    for raw in toml.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            // Section header. Normalize `[dependencies]`, `[dev-dependencies]`,
            // `[build-dependencies]` (ignore `[workspace.dependencies]` etc.).
            section = rest.split('.').next().unwrap_or(rest).trim().to_owned();
            continue;
        }
        if section == "package" {
            if let Some(n) = strip_key(line, "name") {
                name = Some(n);
            }
        } else if matches!(
            section.as_str(),
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) {
            // `dep-name = "1.0"` or `dep-name = { ... }` → key is the dep name.
            if let Some(eq) = line.find('=') {
                let key = line[..eq].trim();
                if is_valid_dep_name(key) {
                    deps.push(key.to_owned());
                }
            }
        }
    }
    (name, deps)
}

/// Parse a package.json into `(name, dependency names)`; malformed JSON → None.
pub fn parse_package_json_deps(json: &str) -> Option<(String, Vec<String>)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let name = v.get("name")?.as_str()?.to_owned();
    let mut deps: Vec<String> = Vec::new();
    for key in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(obj) = v.get(key).and_then(|o| o.as_object()) {
            for k in obj.keys() {
                if is_valid_dep_name(k) {
                    deps.push(k.clone());
                }
            }
        }
    }
    Some((name, deps))
}

/// `key = "value"` → `value` (unquoted), else None.
fn strip_key(line: &str, key: &str) -> Option<String> {
    let eq = line.find('=')?;
    let k = line[..eq].trim();
    if k != key {
        return None;
    }
    let val = line[eq + 1..].trim();
    let val = val.strip_prefix('"').unwrap_or(val);
    let val = val.strip_suffix('"').unwrap_or(val);
    let val = val.trim();
    if val.is_empty() {
        None
    } else {
        Some(val.to_owned())
    }
}

fn is_valid_dep_name(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('_')
        && s.chars().all(|c| {
            c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/' || c == '@'
        })
}

// --- walker ---

fn walk_manifests(root: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    walk_manifests_dir(root, root, &mut files);
    files
}

fn walk_manifests_dir(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.')
                || matches!(
                    name,
                    "node_modules" | "target" | "dist" | "build" | "vendor"
                )
            {
                continue;
            }
            walk_manifests_dir(root, &path, files);
        } else if path.is_file() {
            let base = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if base == "Cargo.toml" || base == "package.json" {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if text.len() > 512 * 1024 {
                        continue;
                    }
                    let rel = path
                        .strip_prefix(root)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    files.push((rel, text));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_parses_dependencies_and_dev_dependencies() {
        let toml = r#"
[package]
name = "engram-domain"
version = "0.1.0"

[dependencies]
serde = "1.0"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
proptest = "1"
"#;
        let (name, deps) = parse_cargo_deps(toml);
        assert_eq!(name.as_deref(), Some("engram-domain"));
        assert!(deps.contains(&"serde".to_owned()));
        assert!(deps.contains(&"chrono".to_owned()));
        assert!(deps.contains(&"proptest".to_owned()));
    }

    #[test]
    fn cargo_workspace_root_has_no_package_name() {
        let toml = r#"
[workspace]
members = ["core/domain"]

[workspace.dependencies]
serde = "1.0"
"#;
        let (name, deps) = parse_cargo_deps(toml);
        // Virtual workspace: no [package], workspace.dependencies ignored.
        assert!(name.is_none());
        assert!(deps.is_empty(), "workspace deps are not consumed: {deps:?}");
    }

    #[test]
    fn cargo_path_dep_key_is_the_dep_name() {
        let toml = r#"
[package]
name = "engram-belief"

[dependencies]
engram-domain = { path = "../domain" }
serde = "1.0"
"#;
        let (name, deps) = parse_cargo_deps(toml);
        assert_eq!(name.as_deref(), Some("engram-belief"));
        assert!(deps.contains(&"engram-domain".to_owned()));
        assert!(deps.contains(&"serde".to_owned()));
    }

    #[test]
    fn package_json_parses_deps_and_dev_deps() {
        let json = r#"{
  "name": "@engram/client",
  "dependencies": { "zod": "^3.0.0" },
  "devDependencies": { "typescript": "^5.0.0" }
}"#;
        let (name, deps) = parse_package_json_deps(json).expect("parses");
        assert_eq!(name, "@engram/client");
        assert!(deps.contains(&"zod".to_owned()));
        assert!(deps.contains(&"typescript".to_owned()));
    }

    #[test]
    fn package_json_missing_name_is_none() {
        let json = r#"{ "dependencies": { "zod": "^3.0.0" } }"#;
        assert!(parse_package_json_deps(json).is_none());
    }

    #[test]
    fn package_json_malformed_is_none() {
        assert!(parse_package_json_deps("{ not json").is_none());
    }
}
