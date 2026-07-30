//! Ownership-boundary extraction (post-index scan).
//!
//! `scan_ownership` walks a repository AFTER `scan_repo` for a `CODEOWNERS`
//! file (`.github/CODEOWNERS`, `CODEOWNERS`, `docs/CODEOWNERS`) and extracts
//! team/individual ownership: each rule becomes `Organization` (team) or
//! `Person` (individual) entities plus an `owns` edge to a path `Module`
//! entity. This adds the "who owns what" layer that turns the graph into a
//! multi-team program view. Mirrors the `scan_protocols` / `scan_dependencies`
//! post-index pattern; routes writes through `require_knowledge()`. A missing
//! CODEOWNERS is a documented no-op, not an error.

use std::path::{Path, PathBuf};

use chrono::Utc;
use engram_domain::{EntityKind, EntityRef, Id, KnowledgeEntity, KnowledgeRelationship};
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::{internal, provenance, req_str};

/// `scan_ownership`: post-index scan that extracts CODEOWNERS rules into
/// Organization/Person entities + `owns` edges to path Modules.
pub fn scan_ownership(app: &App, args: &Value) -> Result<Value, ToolError> {
    let root = req_str(args, "path")?;
    let knowledge = app.provider.require_knowledge().map_err(internal)?;
    let scope = app.scope.clone();
    let now = Utc::now();
    let prov = provenance("mcp-scan-ownership");

    let Some((codeowners_path, text)) = discover_codeowners(Path::new(root)) else {
        return Ok(protocol::text_content(
            "No CODEOWNERS found (.github/CODEOWNERS, CODEOWNERS, docs/CODEOWNERS) — no-op.",
        ));
    };
    let rules = parse_codeowners(&text);
    if rules.is_empty() {
        return Ok(protocol::text_content(format!(
            "CODEOWNERS at {codeowners_path} had no rules — no-op.",
        )));
    }

    // Distinct owners + distinct paths for entity creation.
    let mut owners: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut edges: Vec<(String, String)> = Vec::new(); // (owner, path)
    for (path, owner_list) in &rules {
        paths.insert(path.clone());
        for owner in owner_list {
            owners.insert(owner.clone());
            edges.push((owner.clone(), path.clone()));
        }
    }

    // Persist owner entities (Organization if team-like "org/team", else Person).
    for owner in &owners {
        let kind = if owner.contains('/') {
            EntityKind::Organization
        } else {
            EntityKind::Person
        };
        let entity = KnowledgeEntity {
            id: Id::from(format!("owner-{owner}")),
            graph_id: None,
            kind,
            name: owner.clone(),
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
    }

    // Persist path Module entities.
    for path in &paths {
        let entity = KnowledgeEntity {
            id: Id::from(format!("path-{path}")),
            graph_id: None,
            kind: EntityKind::Module,
            name: path.clone(),
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
    }

    // Persist owns edges.
    for (owner, path) in &edges {
        let rel = KnowledgeRelationship {
            id: Id::from(format!("{owner}\u{1f}owns\u{1f}{path}")),
            graph_id: None,
            subject: EntityRef {
                id: Some(Id::from(format!("owner-{owner}"))),
                kind: Some("owner".to_owned()),
                name: Some(owner.clone()),
                aliases: Vec::new(),
            },
            predicate: "owns".to_owned(),
            object: EntityRef {
                id: Some(Id::from(format!("path-{path}"))),
                kind: Some("module".to_owned()),
                name: Some(path.clone()),
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
    }

    Ok(protocol::text_content(format!(
        "Ownership scan: {codeowners_path}, {} owner entities, {} path entities, {} owns edges.",
        owners.len(),
        paths.len(),
        edges.len(),
    )))
}

// --- pure parser + discovery ---

/// Parse CODEOWNERS text into `(path, owner_names)` rules. Comments (`#`) and
/// blank lines are skipped; the first token of a rule is its path, the rest are
/// `@owner` tokens (leading `@` stripped).
pub fn parse_codeowners(text: &str) -> Vec<(String, Vec<String>)> {
    let mut rules = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(path) = parts.next() else {
            continue;
        };
        let owners: Vec<String> = parts
            .filter(|t| t.starts_with('@'))
            .map(|t| t.trim_start_matches('@').to_owned())
            .filter(|t| !t.is_empty())
            .collect();
        if !owners.is_empty() {
            rules.push((path.to_owned(), owners));
        }
    }
    rules
}

/// Discover the first CODEOWNERS file under `root`; returns its rel path + text.
fn discover_codeowners(root: &Path) -> Option<(String, String)> {
    let candidates: [PathBuf; 3] = [
        root.join(".github").join("CODEOWNERS"),
        root.join("CODEOWNERS"),
        root.join("docs").join("CODEOWNERS"),
    ];
    for c in candidates {
        if let Ok(text) = std::fs::read_to_string(&c) {
            let rel = c
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| c.to_string_lossy().to_string());
            return Some((rel, text));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codeowners_parses_rules_teams_and_individuals() {
        let text = r#"
# This is a comment.
* @acme/root
/src/ @acme/backend @alice
*.md @bob
"#;
        let rules = parse_codeowners(text);
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].0, "*");
        assert_eq!(rules[0].1, vec!["acme/root".to_owned()]);
        assert_eq!(rules[1].0, "/src/");
        assert_eq!(
            rules[1].1,
            vec!["acme/backend".to_owned(), "alice".to_owned()]
        );
        assert_eq!(rules[2].0, "*.md");
        assert_eq!(rules[2].1, vec!["bob".to_owned()]);
    }

    #[test]
    fn codeowners_skips_rules_without_owners() {
        let text = "/orphan/path\n/has-owner/ @team\n";
        let rules = parse_codeowners(text);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].0, "/has-owner/");
    }

    #[test]
    fn codeowners_empty_is_empty() {
        assert!(parse_codeowners("").is_empty());
        assert!(parse_codeowners("# just a comment\n   \n").is_empty());
    }

    #[test]
    fn codeowners_strips_leading_at_only() {
        let rules = parse_codeowners("/x/ @@weird @normal\n");
        // '@@weird' → 'weird' (one '@' stripped); '@normal' → 'normal'.
        assert_eq!(rules[0].1, vec!["weird".to_owned(), "normal".to_owned()]);
    }
}
