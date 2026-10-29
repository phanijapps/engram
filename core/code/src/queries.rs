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
            code_resolution: None,
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

/// Explore node carrying the STABLE entity id alongside its display label
/// (corrective r2, finding 10): same-named symbols in different files — or
/// identical paths in different repositories — stay distinct nodes because
/// the traversal keys on the id, never on the name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExploreNodeById {
    pub id: String,
    pub label: String,
    pub kind: Option<String>,
    pub hop: u32,
}

/// Id-keyed bounded BFS over `calls`/`contains` edges (finding 10).
///
/// `entity_nodes` is a list of `(id, label, kind)` triples; edges are given
/// as `(subject_id, object_id, predicate)` so NO name resolution happens
/// inside the traversal. Seeds are the entities whose label tail matches a
/// query token (same matching as [`explore`]), keyed by id; two entities with
/// the same label remain distinct nodes. Budgets (`max_nodes`, `max_edges`)
/// apply to the authorized edge set the caller already constrained.
pub fn explore_by_id(
    edges: &[(&str, &str, &str)],
    entity_nodes: &[(String, String, Option<String>)],
    query: &str,
    max_depth: u32,
    max_nodes: usize,
    max_edges: usize,
) -> Vec<ExploreNodeById> {
    let tokens: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_lowercase())
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut adjacency: std::collections::HashMap<&str, Vec<&str>> = Default::default();
    for (subject, object, predicate) in edges
        .iter()
        .filter(|(_, _, predicate)| *predicate == "calls" || *predicate == "contains")
    {
        adjacency.entry(subject).or_default().push(object);
    }
    let mut seeds: Vec<&str> = Vec::new();
    for (id, label, _) in entity_nodes {
        let lower = label.to_lowercase();
        let tail = lower.rsplit("::").next().unwrap_or(&lower);
        if tokens
            .iter()
            .any(|t| t == tail || (tail.len() < 40 && tail.contains(t.as_str())))
        {
            seeds.push(id.as_str());
        }
    }
    if seeds.is_empty() {
        return Vec::new();
    }
    let kind_by_id: std::collections::HashMap<&str, &Option<String>> = entity_nodes
        .iter()
        .map(|(id, _, kind)| (id.as_str(), kind))
        .collect();
    let label_by_id: std::collections::HashMap<&str, &str> = entity_nodes
        .iter()
        .map(|(id, label, _)| (id.as_str(), label.as_str()))
        .collect();
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<&str> = Default::default();
    let mut edge_count = 0usize;
    let mut frontier: std::collections::VecDeque<(&str, u32)> =
        seeds.into_iter().map(|id| (id, 0)).collect();
    while let Some((id, hop)) = frontier.pop_front() {
        if out.len() >= max_nodes || !seen.insert(id) {
            continue;
        }
        out.push(ExploreNodeById {
            id: id.to_owned(),
            label: label_by_id.get(id).copied().unwrap_or_default().to_owned(),
            kind: kind_by_id.get(id).and_then(|k| (*k).clone()),
            hop,
        });
        if hop >= max_depth {
            continue;
        }
        if let Some(neighbors) = adjacency.get(id) {
            for next in neighbors {
                if edge_count >= max_edges {
                    break;
                }
                edge_count += 1;
                frontier.push_back((next, hop + 1));
            }
        }
    }
    out.sort_by_key(|node| node.hop);
    out
}

/// File-dependency edge with the ALREADY-RESOLVED target preserved (finding
/// 10): when a relationship's object endpoint identifies a known file entity,
/// its canonical path wins over suffix re-resolution — an import that
/// explicitly targets repo B's `src/util.rs` stays resolved even when repo A
/// has an identical path (the pure suffix resolver would call that ambiguous).
pub fn file_dependencies_resolved(
    relationships: &[KnowledgeRelationship],
    file_paths: &[String],
    object_id_to_path: &std::collections::HashMap<String, String>,
) -> Vec<FileDependency> {
    let path_set: std::collections::HashSet<&str> = file_paths.iter().map(String::as_str).collect();
    let mut out = Vec::new();
    for rel in relationships.iter().filter(|r| r.predicate == "imports") {
        let (Some(from), Some(import)) = (rel.subject.name.as_deref(), rel.object.name.as_deref())
        else {
            continue;
        };
        let resolved = rel
            .object
            .id
            .as_ref()
            .and_then(|id| object_id_to_path.get(id.as_str()))
            .filter(|path| path_set.contains(path.as_str()))
            .cloned()
            .or_else(|| resolve_module_path(import, file_paths));
        out.push(FileDependency {
            from_path: from.to_owned(),
            import_path: import.to_owned(),
            resolved_to: resolved,
        });
    }
    out.sort_by(|a, b| (&a.from_path, &a.import_path).cmp(&(&b.from_path, &b.import_path)));
    out.dedup_by(|a, b| a.from_path == b.from_path && a.import_path == b.import_path);
    out
}

#[cfg(test)]
mod r2_tests {
    use super::*;
    use engram_domain::{
        Actor, ActorKind, AllowedUse, EntityRef, Id, Policy, Provenance, Retention, Scope,
        Visibility,
    };

    fn rel(
        id: &str,
        predicate: &str,
        subject_id: &str,
        subject_name: &str,
        object_id: &str,
        object_name: &str,
    ) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: Id::from(id),
            graph_id: None,
            predicate: predicate.to_owned(),
            subject: EntityRef {
                id: Some(Id::from(subject_id)),
                kind: None,
                name: Some(subject_name.to_owned()),
                aliases: Vec::new(),
            },
            object: EntityRef {
                id: Some(Id::from(object_id)),
                kind: None,
                name: Some(object_name.to_owned()),
                aliases: Vec::new(),
            },
            scope: Scope {
                tenant: "t".into(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            provenance: Provenance {
                source: "q".into(),
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
            evidence: Vec::new(),
            confidence: None,
            created_at: chrono::Utc::now(),
            updated_at: None,
            code_resolution: None,
            archived_at: None,
        }
    }

    /// Finding 10: same-named symbols in different files of ONE repository
    /// stay distinct explore nodes because traversal keys on entity ids.
    #[test]
    fn same_name_different_file_symbols_stay_distinct() {
        let edge_records = vec![
            ("e1", "calls", "fn-1", "render", "hub", "hub"),
            ("e2", "calls", "fn-2", "render", "hub", "hub"),
        ];
        let edges: Vec<(&str, &str, &str)> = edge_records
            .iter()
            .map(|(_, predicate, subject, _, object, _)| (*predicate, *subject, *object))
            .map(|(predicate, subject, object)| (subject, object, predicate))
            .collect();
        let nodes = vec![
            (
                "fn-1".to_owned(),
                "src/a.rs::render".to_owned(),
                Some("function".to_owned()),
            ),
            (
                "fn-2".to_owned(),
                "src/b.rs::render".to_owned(),
                Some("function".to_owned()),
            ),
            ("hub".to_owned(), "hub".to_owned(), None),
        ];
        let out = explore_by_id(&edges, &nodes, "render", 1, 24, 64);
        let ids: Vec<&str> = out.iter().map(|node| node.id.as_str()).collect();
        assert!(
            ids.contains(&"fn-1") && ids.contains(&"fn-2"),
            "distinct ids stay distinct: {ids:?}"
        );
        assert_eq!(
            out.iter()
                .filter(|node| node.label.ends_with("::render"))
                .count(),
            2,
            "both same-named symbols appear as separate nodes"
        );
    }

    /// Finding 10: an import whose object endpoint already identifies repo B's
    /// file stays resolved even when repo A has the identical path.
    #[test]
    fn resolved_import_target_survives_identical_paths() {
        let paths = vec![
            "repo-a::src/util.rs".to_owned(),
            "repo-b::src/util.rs".to_owned(),
        ];
        let relationships = vec![rel(
            "r-1",
            "imports",
            "file-a",
            "repo-a::src/app.rs",
            "file-b",
            "./util",
        )];
        let mut object_map = std::collections::HashMap::new();
        object_map.insert("file-b".to_owned(), "repo-b::src/util.rs".to_owned());
        let out = file_dependencies_resolved(&relationships, &paths, &object_map);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].resolved_to.as_deref(),
            Some("repo-b::src/util.rs"),
            "the scanner-resolved endpoint wins over ambiguous suffix resolution"
        );
        // Without the resolved endpoint the pure resolver honestly reports
        // ambiguity between identical stems.
        let unresolved = file_dependencies(&relationships, &paths);
        assert_eq!(unresolved[0].resolved_to, None);
    }
}
