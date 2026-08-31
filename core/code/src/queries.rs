//! Phase-2 query compositions over extracted code facts: the file-level
//! dependency graph (`imports` rollup with module→file resolution) and the
//! natural-language `explore` entry point (token seeding + bounded BFS).
//! Pure functions over domain records — the same implementation serves the
//! Rust facade, both MCP servers, and the N-API binding.

use engram_domain::KnowledgeRelationship;

// ── RFC-0020 Phase 2: file dependencies + explore (engram-code spec T10) ────

/// One file-level dependency edge: source file → the module path it imports,
/// resolved to the defining file when the scanned source contains a file
/// whose stem matches the import's last segment (best-effort suffix match;
/// unresolved module paths keep `None`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDependency {
    pub from_path: String,
    pub import_path: String,
    pub resolved_to: Option<String>,
}

/// File-level dependency graph from `imports` edges: File entities → Module
/// entities. Module paths resolve against the set of known file paths by
/// stem suffix (`./utils` → `src/utils.rs`).
pub fn file_dependencies(
    relationships: &[KnowledgeRelationship],
    file_paths: &[String],
) -> Vec<FileDependency> {
    let mut out = Vec::new();
    for rel in relationships.iter().filter(|r| r.predicate == "imports") {
        let (Some(from), Some(import)) = (rel.subject.name.as_deref(), rel.object.name.as_deref())
        else {
            continue;
        };
        let resolved = resolve_module_path(import, file_paths);
        out.push(FileDependency {
            from_path: from.to_owned(),
            import_path: import.to_owned(),
            resolved_to: resolved,
        });
    }
    out
}

/// Resolve an import path to a known file by stem suffix. `./utils` matches
/// `src/utils.rs` (any extension); `std::fmt` matches nothing local.
fn resolve_module_path(import: &str, file_paths: &[String]) -> Option<String> {
    let stem = import
        .trim_start_matches("./")
        .trim_start_matches("../")
        .rsplit('/')
        .next()
        .unwrap_or(import);
    if stem.is_empty() {
        return None;
    }
    let matches: Vec<&String> = file_paths
        .iter()
        .filter(|path| {
            let file_stem = path.rsplit('/').next().unwrap_or(path);
            let without_ext = file_stem.split('.').next().unwrap_or(file_stem);
            without_ext == stem
        })
        .collect();
    // Ambiguity stays honest: multiple same-stem files in different
    // directories do not silently pick a winner (spec principle) — the
    // dependency stays unresolved.
    if matches.len() == 1 {
        Some(matches[0].clone())
    } else {
        None
    }
}

/// One node of an `explore` result: an entity matched by the query seed or
/// reached by bounded expansion.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreNode {
    pub name: String,
    pub kind: Option<String>,
    /// hop distance from the seed set (0 = seed match).
    pub hop: u32,
}

/// Relevance-seeded bounded subgraph for a natural-language query
/// (RFC-0020 Phase 2 `explore`): the query's identifier-shaped tokens match
/// entity names (bare tail or substring, case-insensitive); matched seeds
/// expand over `calls`/`contains` edges up to `max_depth` hops, capped at
/// `max_nodes` nodes and `max_edges` edges. Deterministic, read-only.
pub fn explore(
    relationships: &[KnowledgeRelationship],
    entity_names: &[(String, Option<String>)], // (name, kind)
    query: &str,
    max_depth: u32,
    max_nodes: usize,
    max_edges: usize,
) -> Vec<ExploreNode> {
    // Identifier-shaped tokens from the query (len >= 3 filters stopwords).
    let tokens: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_lowercase())
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    // Seed match: token equals the entity's bare tail or is contained in it.
    let mut seeds: Vec<(String, Option<String>)> = Vec::new();
    for (name, kind) in entity_names {
        let lower = name.to_lowercase();
        let tail = lower.rsplit("::").next().unwrap_or(&lower);
        if tokens
            .iter()
            .any(|t| t == tail || (tail.len() < 40 && tail.contains(t.as_str())))
        {
            seeds.push((name.clone(), kind.clone()));
        }
    }
    if seeds.is_empty() {
        return Vec::new();
    }
    // Bounded BFS over call/contains edges, keyed by entity name.
    let adjacency: std::collections::HashMap<String, Vec<String>> = {
        let mut map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for rel in relationships
            .iter()
            .filter(|r| r.predicate == "calls" || r.predicate == "contains")
        {
            if let (Some(s), Some(o)) = (rel.subject.name.as_deref(), rel.object.name.as_deref()) {
                map.entry(s.to_owned()).or_default().push(o.to_owned());
            }
        }
        map
    };
    let kind_by_name: std::collections::HashMap<String, Option<String>> = entity_names
        .iter()
        .cloned()
        .collect::<std::collections::HashMap<_, _>>();
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut edge_count = 0usize;
    let mut frontier: std::collections::VecDeque<(String, u32)> = seeds
        .iter()
        .map(|(name, _kind)| (name.clone(), 0))
        .collect();
    while let Some((name, hop)) = frontier.pop_front() {
        if out.len() >= max_nodes || !seen.insert(name.clone()) {
            continue;
        }
        out.push(ExploreNode {
            kind: kind_by_name.get(&name).cloned().flatten(),
            name: name.clone(),
            hop,
        });
        if hop >= max_depth {
            continue;
        }
        if let Some(neighbors) = adjacency.get(&name) {
            for next in neighbors {
                if edge_count >= max_edges {
                    break;
                }
                edge_count += 1;
                frontier.push_back((next.clone(), hop + 1));
            }
        }
    }
    out.sort_by_key(|n| n.hop);
    out
}

#[cfg(test)]
mod phase2_query_tests {
    use super::*;
    use engram_domain::*;

    fn rel(predicate: &str, subject: &str, object: &str) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: RelationshipId::from("r"),
            graph_id: None,
            subject: EntityRef {
                id: None,
                kind: None,
                name: Some(subject.to_owned()),
                aliases: Vec::new(),
            },
            predicate: predicate.to_owned(),
            object: EntityRef {
                id: None,
                kind: None,
                name: Some(object.to_owned()),
                aliases: Vec::new(),
            },
            scope: Scope {
                tenant: "t".to_owned(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            evidence: Vec::new(),
            confidence: None,
            provenance: Provenance {
                source: "test".to_owned(),
                actor: Actor {
                    id: Id::from("a"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: None,
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            archived_at: None,
        }
    }

    #[test]
    fn file_dependencies_resolves_stems() {
        let rels = vec![
            rel("imports", "src/api.ts", "./utils"),
            rel("imports", "src/main.rs", "std::fmt"),
        ];
        let files = vec!["src/utils.ts".to_owned(), "src/main.rs".to_owned()];
        let deps = file_dependencies(&rels, &files);
        assert_eq!(deps.len(), 2);
        let utils = deps.iter().find(|d| d.import_path == "./utils").unwrap();
        assert_eq!(utils.resolved_to.as_deref(), Some("src/utils.ts"));
        let std_fmt = deps.iter().find(|d| d.import_path == "std::fmt").unwrap();
        assert!(
            std_fmt.resolved_to.is_none(),
            "external import stays unresolved"
        );
    }

    #[test]
    fn explore_seeds_on_tokens_and_expands() {
        let rels = vec![
            rel("calls", "parse_config", "validate"),
            rel("contains", "Config", "Config::parse"),
        ];
        let names = vec![
            ("parse_config".to_owned(), Some("function".to_owned())),
            ("validate".to_owned(), None),
            ("Config::parse".to_owned(), None),
        ];
        let nodes = explore(&rels, &names, "how does parse_config work", 2, 24, 64);
        assert!(nodes.iter().any(|n| n.name == "parse_config" && n.hop == 0));
        assert!(nodes.iter().any(|n| n.name == "validate" && n.hop == 1));
    }

    #[test]
    fn explore_budget_is_enforced() {
        let rels: Vec<_> = (0..100)
            .map(|i| rel("calls", "parse_config", &format!("fn_{i}")))
            .collect();
        let names = vec![("parse_config".to_owned(), None)];
        let nodes = explore(&rels, &names, "parse_config", 1, 5, 4);
        assert!(nodes.len() <= 5, "node budget: {}", nodes.len());
    }

    #[test]
    fn explore_with_no_identifier_tokens_is_empty() {
        let nodes = explore(&[], &[], "how does it work?", 2, 24, 64);
        assert!(nodes.is_empty());
    }
}
