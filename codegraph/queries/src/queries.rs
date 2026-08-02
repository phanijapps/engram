//! Map knowledge-graph call edges to a generic edge list and answer
//! code-specific queries over them.

use std::collections::{HashMap, HashSet};

use engram_domain::{EntityRef, KnowledgeRelationship};

/// Stable string key for an entity reference: its **name** (human-readable, what
/// queries + callers use), else its resolved id. Returns `None` for a ref with
/// neither. Name is preferred because graph queries (`symbol_context`,
/// `blast_radius`, …) are invoked by function NAME, and scanner entity IDs are
/// opaque hashes (`entity-…`) that would never match a name-based query.
pub fn entity_key(reference: &EntityRef) -> Option<String> {
    if let Some(name) = &reference.name {
        return Some(name.clone());
    }
    reference.id.as_ref().map(|id| id.as_str().to_owned())
}

/// Extracts `(caller, callee)` string pairs from `calls` relationships.
/// Other predicates and refs without a key are skipped.
pub fn call_edges(relationships: &[KnowledgeRelationship]) -> Vec<(String, String)> {
    relationships
        .iter()
        .filter(|r| {
            matches!(
                r.predicate.as_str(),
                "calls" | "sends_request" | "handled_by"
            )
        })
        .filter_map(|r| {
            let caller = entity_key(&r.subject)?;
            let callee = entity_key(&r.object)?;
            Some((caller, callee))
        })
        .collect()
}

/// Returns the dead-code set: symbols in the call graph with zero callers
/// (zero in-degree on `calls` edges), sorted for determinism.
///
/// Note: entry points (main, HTTP handlers) also have zero callers and surface
/// here — callers filter known entry points. Mirrors memtrace's `find_dead_code`.
pub fn dead_code(relationships: &[KnowledgeRelationship]) -> Vec<String> {
    let edges = call_edges(relationships);
    let in_degree = engram_graph_analytics::in_degree(&edges);
    let mut defined: HashSet<String> = HashSet::new();
    for (caller, callee) in &edges {
        defined.insert(caller.clone());
        defined.insert(callee.clone());
    }
    let mut dead: Vec<String> = defined
        .into_iter()
        .filter(|node| !in_degree.contains_key(node))
        .collect();
    dead.sort();
    dead
}

/// Returns the blast radius of `target`: its transitive callers within `depth`
/// hops (reverse reachability over `calls` edges). Empty if `target` is unknown.
pub fn blast_radius(
    relationships: &[KnowledgeRelationship],
    target: &str,
    depth: usize,
) -> HashSet<String> {
    let edges = call_edges(relationships);
    engram_graph_analytics::ancestors(&edges, &target.to_owned(), depth)
}

/// Returns the shortest dependency path `from -> to` along `calls` edges
/// (inclusive endpoints), or `None` if unreachable.
pub fn dependency_path(
    relationships: &[KnowledgeRelationship],
    from: &str,
    to: &str,
) -> Option<Vec<String>> {
    let edges = call_edges(relationships);
    engram_graph_analytics::shortest_path(&edges, &from.to_owned(), &to.to_owned())
}

/// Returns the most central symbols (PageRank over `calls` edges), best-first.
/// Mirrors memtrace's `find_central_symbols` — the functions/classes most other
/// code depends on.
pub fn central_symbols(
    relationships: &[KnowledgeRelationship],
    limit: usize,
) -> Vec<(String, f64)> {
    let edges = call_edges(relationships);
    let mut ranked: Vec<(String, f64)> = engram_graph_analytics::pagerank(&edges, 0.85, 100, 1e-6)
        .into_iter()
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(limit);
    ranked
}

/// Returns the highest-betweenness symbols over `calls` edges, best-first — the
/// chokepoints. Touching these has outsized blast radius. Mirrors memtrace's
/// `find_bridge_symbols`.
pub fn bridge_symbols(relationships: &[KnowledgeRelationship], limit: usize) -> Vec<(String, f64)> {
    let edges = call_edges(relationships);
    let mut ranked: Vec<(String, f64)> = engram_graph_analytics::betweenness(&edges)
        .into_iter()
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(limit);
    ranked
}

/// Returns the community label per symbol (Louvain over `calls` edges). Mirrors
/// memtrace's `list_communities` — clusters of tightly-coupled symbols.
pub fn call_communities(
    relationships: &[KnowledgeRelationship],
    max_passes: usize,
) -> HashMap<String, usize> {
    let edges = call_edges(relationships);
    engram_graph_analytics::communities(&edges, max_passes)
}

/// A 360° view of one symbol: its transitive callers, transitive callees, and
/// Louvain community label. Mirrors memtrace's `get_symbol_context`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SymbolContext {
    pub callers: Vec<String>,
    pub callees: Vec<String>,
    pub community: Option<usize>,
}

/// Returns the 360° context of `symbol`: transitive callers (blast radius),
/// transitive callees, and its community label.
pub fn symbol_context(
    relationships: &[KnowledgeRelationship],
    symbol: &str,
    depth: usize,
) -> SymbolContext {
    let edges = call_edges(relationships);
    let mut callers: Vec<String> =
        engram_graph_analytics::ancestors(&edges, &symbol.to_owned(), depth)
            .into_iter()
            .collect();
    callers.sort();
    let mut callees: Vec<String> =
        engram_graph_analytics::descendants(&edges, &symbol.to_owned(), depth)
            .into_iter()
            .collect();
    callees.sort();
    let community = engram_graph_analytics::communities(&edges, 20)
        .get(symbol)
        .copied();
    SymbolContext {
        callers,
        callees,
        community,
    }
}

/// A bounded 360° view of one symbol: the [`SymbolContext`] plus a `truncated`
/// flag that is `true` iff the visited cap cut off either the callers (ancestors)
/// or callees (descendants) direction. Additive sibling of [`symbol_context`];
/// the original `symbol_context` / [`SymbolContext`] are unchanged.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SymbolContextBounded {
    pub ctx: SymbolContext,
    pub truncated: bool,
}

/// Like [`symbol_context`], but bounds callers and callees each to at most `cap`
/// nodes (per-direction) and reports whether either direction was truncated.
/// `depth` is the outer hop bound; `cap` is the inner visited bound (safety net
/// for raised-depth or super-hub queries). `truncated` is the OR of the two
/// directions' truncation flags.
pub fn symbol_context_bounded(
    relationships: &[KnowledgeRelationship],
    symbol: &str,
    depth: usize,
    cap: usize,
) -> SymbolContextBounded {
    let edges = call_edges(relationships);
    let (anc, anc_truncated) =
        engram_graph_analytics::ancestors_bounded(&edges, &symbol.to_owned(), depth, cap);
    let (desc, desc_truncated) =
        engram_graph_analytics::descendants_bounded(&edges, &symbol.to_owned(), depth, cap);
    let mut callers: Vec<String> = anc.into_iter().collect();
    callers.sort();
    let mut callees: Vec<String> = desc.into_iter().collect();
    callees.sort();
    let community = engram_graph_analytics::communities(&edges, 20)
        .get(symbol)
        .copied();
    SymbolContextBounded {
        ctx: SymbolContext {
            callers,
            callees,
            community,
        },
        truncated: anc_truncated || desc_truncated,
    }
}

/// A bounded blast radius: the transitive callers of a target, capped, plus a
/// `truncated` flag. Additive sibling of [`blast_radius`]; the original
/// `blast_radius` (returning `HashSet<String>`) is unchanged so its N-API JSON
/// consumer is unaffected.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BlastRadiusBounded {
    pub callers: Vec<String>,
    pub truncated: bool,
}

/// Like [`blast_radius`], but bounds the callers to at most `cap` nodes and
/// reports whether the cap cut off further exploration. `callers` is sorted for
/// determinism (mirrors [`SymbolContext::callers`]).
pub fn blast_radius_bounded(
    relationships: &[KnowledgeRelationship],
    target: &str,
    depth: usize,
    cap: usize,
) -> BlastRadiusBounded {
    let edges = call_edges(relationships);
    let (anc, truncated) =
        engram_graph_analytics::ancestors_bounded(&edges, &target.to_owned(), depth, cap);
    let mut callers: Vec<String> = anc.into_iter().collect();
    callers.sort();
    BlastRadiusBounded { callers, truncated }
}

/// Estimates cyclomatic complexity from source text: 1 + count of decision-point
/// patterns (`if`/`for`/`while`/`match`/`switch`/`case`/`catch` + `&&`/`||`).
/// A language-agnostic text heuristic — not AST-precise, but useful for ranking
/// refactoring candidates. Mirrors memtrace's `calculate_cyclomatic_complexity`.
pub fn cyclomatic_complexity(source: &str) -> usize {
    let mut decisions = 1usize;
    for line in source.lines() {
        let trimmed = line.trim_start();
        // Skip comment lines (rough heuristic — language-agnostic).
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('*') {
            continue;
        }
        for pattern in [
            "if ", "if(", "for ", "for(", "while ", "while(", "match ", "match(", "switch ",
            "switch(", "case ", "catch ", "catch(",
        ] {
            decisions += trimmed.matches(pattern).count();
        }
        decisions += trimmed.matches("&&").count();
        decisions += trimmed.matches("||").count();
    }
    decisions
}

/// A detected HTTP endpoint: method + path.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HttpEndpoint {
    pub method: String,
    pub path: String,
}

/// Extracts HTTP endpoints from source text by matching framework route patterns
/// (`.get("/path")`, `@app.post("/path")`, `#[get("/path")]`, etc.). A
/// language-agnostic text heuristic — detects Express, FastAPI, Flask, Actix,
/// Gin and similar. Spring `@GetMapping` is a follow-up. Mirrors memtrace's
/// `find_api_endpoints`.
pub fn find_endpoints(source: &str) -> Vec<HttpEndpoint> {
    let methods = ["get", "post", "put", "delete", "patch"];
    let mut endpoints = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || (trimmed.starts_with('#') && !trimmed.starts_with("#[")) {
            continue;
        }
        let lower = trimmed.to_lowercase();
        for method in methods {
            for quote in ['"', '\''] {
                let pattern = format!("{method}({quote}");
                if let Some(pos) = lower.find(&pattern) {
                    // Must be preceded by a non-alphanumeric char (route pattern,
                    // not a function call like budget( or target().
                    if pos > 0 && trimmed.as_bytes()[pos - 1].is_ascii_alphanumeric() {
                        continue;
                    }
                    let after = &trimmed[pos + method.len() + 1..]; // after "method("
                    if let Some(path) = extract_quoted(after, quote) {
                        endpoints.push(HttpEndpoint {
                            method: method.to_uppercase(),
                            path,
                        });
                    }
                }
            }
        }
    }
    endpoints
}

/// Extracts the content between the first pair of `quote` chars in `s`.
fn extract_quoted(s: &str, quote: char) -> Option<String> {
    let start = s.find(quote)?;
    let rest = &s[start + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_owned())
}

/// Extracts HTTP call-site targets from source text — paths/URLs passed to
/// `fetch(...)`, `axios.METHOD(...)`, `requests.METHOD(...)`, `http.METHOD(...)`.
/// Pure text heuristic; the caller context (which entity owns the call site) is
/// determined at the wiring layer. Mirrors memtrace's `find_api_calls`.
pub fn find_api_calls(source: &str) -> Vec<String> {
    let call_patterns = [
        "fetch(",
        "axios.get(",
        "axios.post(",
        "axios.put(",
        "axios.delete(",
        "axios.patch(",
        "requests.get(",
        "requests.post(",
        "requests.put(",
        "requests.delete(",
        "requests.patch(",
        "http.Get(",
        "http.Post(",
        "http.Put(",
        "http.Delete(",
    ];
    let mut calls = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || (trimmed.starts_with('#') && !trimmed.starts_with("#[")) {
            continue;
        }
        for pattern in &call_patterns {
            if let Some(pos) = trimmed.find(pattern) {
                if pos > 0 && trimmed.as_bytes()[pos - 1].is_ascii_alphanumeric() {
                    continue;
                }
                let after = &trimmed[pos + pattern.len()..];
                for quote in ['"', '\''] {
                    if let Some(path) = extract_quoted(after, quote) {
                        if !path.is_empty() {
                            calls.push(path);
                        }
                        break;
                    }
                }
            }
        }
    }
    calls
}

/// Detects entry-point function names from source text (text heuristic).
/// Recognises `fn main(`, `int main(`, `void main(`, `def main(`,
/// `if __name__ == "__main__"`, and `exports.handler`. Mirrors memtrace's
/// `list_processes` (entry-point auto-detection).
pub fn find_entry_points(source: &str) -> Vec<String> {
    let mut entries = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.contains("fn main(")
            || trimmed.contains("int main(")
            || trimmed.contains("void main(")
            || trimmed.contains("def main(")
        {
            entries.push("main".to_owned());
        }
        if trimmed.contains("__name__") && trimmed.contains("__main__") {
            entries.push("__main__".to_owned());
        }
        if trimmed.contains("exports.handler") {
            entries.push("handler".to_owned());
        }
    }
    entries.sort();
    entries.dedup();
    entries
}

/// Traces the execution flow from `entry_point` through the call graph: the
/// entry point followed by all symbols reachable via `calls` edges within
/// `max_depth` hops (sorted for determinism). Mirrors memtrace's
/// `get_process_flow`.
pub fn process_flow(
    relationships: &[KnowledgeRelationship],
    entry_point: &str,
    max_depth: usize,
) -> Vec<String> {
    let edges = call_edges(relationships);
    let mut callees: Vec<String> =
        engram_graph_analytics::descendants(&edges, &entry_point.to_owned(), max_depth)
            .into_iter()
            .collect();
    callees.sort();
    let mut flow = vec![entry_point.to_owned()];
    flow.append(&mut callees);
    flow
}

/// Matches HTTP call-site paths to endpoint definitions, producing cross-service
/// topology edges. Given endpoints (from [`find_endpoints`]) and call paths
/// (from [`find_api_calls`]), returns `(call_path, "METHOD /path")` pairs where
/// the call path matches an endpoint. Mirrors memtrace's `get_api_topology`.
pub fn match_api_topology(endpoints: &[HttpEndpoint], calls: &[String]) -> Vec<(String, String)> {
    let mut matches = Vec::new();
    for call_path in calls {
        for endpoint in endpoints {
            if paths_match(call_path, &endpoint.path) {
                matches.push((
                    call_path.clone(),
                    format!("{} {}", endpoint.method, endpoint.path),
                ));
            }
        }
    }
    matches.sort();
    matches
}

/// Checks if a call path matches an endpoint path (exact or suffix match,
/// ignoring query strings and trailing slashes). Endpoint paths start with `/`,
/// so suffix matching respects path-segment boundaries naturally.
fn paths_match(call_path: &str, endpoint_path: &str) -> bool {
    let call_base = call_path
        .split('?')
        .next()
        .unwrap_or(call_path)
        .trim_end_matches('/');
    let endpoint_base = endpoint_path.trim_end_matches('/');
    call_base == endpoint_base || call_base.ends_with(endpoint_base)
}

/// Resolves name-only call targets against a global name→entity-id index (C1).
/// Fills `object.id` on `calls` relationships whose object is a name-only ref,
/// where the name uniquely maps to an id. Returns the count resolved.
pub fn resolve_call_targets(
    relationships: &mut [KnowledgeRelationship],
    name_to_id: &std::collections::HashMap<String, String>,
) -> usize {
    let mut resolved = 0;
    for rel in relationships.iter_mut() {
        if rel.predicate != "calls" || rel.object.id.is_some() {
            continue;
        }
        if let Some(name) = &rel.object.name {
            if let Some(id) = name_to_id.get(name) {
                rel.object.id = Some(engram_domain::Id::from(id.clone()));
                resolved += 1;
            }
        }
    }
    resolved
}

/// Ranks functions by cyclomatic complexity, most-complex-first. Given
/// `(name, source_text)` pairs, computes complexity per function + truncates to
/// `limit`. Mirrors memtrace's `find_most_complex_functions`.
pub fn most_complex(sources: &[(String, String)], limit: usize) -> Vec<(String, usize)> {
    let mut ranked: Vec<(String, usize)> = sources
        .iter()
        .map(|(name, source)| (name.clone(), cyclomatic_complexity(source)))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    ranked.truncate(limit);
    ranked
}

/// Headline statistics for a call graph: node + edge counts. Mirrors memtrace's
/// `get_repository_stats`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RepositoryStats {
    pub node_count: usize,
    pub edge_count: usize,
}

/// Returns node + edge counts over `calls` relationships.
pub fn repository_stats(relationships: &[KnowledgeRelationship]) -> RepositoryStats {
    let edges = call_edges(relationships);
    let mut nodes: HashSet<String> = HashSet::new();
    for (caller, callee) in &edges {
        nodes.insert(caller.clone());
        nodes.insert(callee.clone());
    }
    RepositoryStats {
        node_count: nodes.len(),
        edge_count: edges.len(),
    }
}

/// Renders a cross-service topology as a Mermaid `graph LR` diagram. Takes the
/// output of [`match_api_topology`] (call-path → endpoint pairs) and produces a
/// Mermaid edge list. Mirrors memtrace's `get_service_diagram`.
pub fn service_diagram(topology: &[(String, String)]) -> String {
    let mut mermaid = String::from("graph LR\n");
    for (call_path, endpoint) in topology {
        let from = call_path.split('?').next().unwrap_or(call_path);
        mermaid.push_str(&format!("  \"{from}\" --> \"{endpoint}\"\n"));
    }
    mermaid
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use engram_domain::{Actor, ActorKind, Id, Provenance, Scope};

    #[test]
    fn call_edges_keeps_calls_drops_others_and_unresolved() {
        let rels = vec![
            rel("a", "b"),
            rel("c", "d"),
            // non-`calls` predicate -> dropped
            KnowledgeRelationship {
                predicate: "imports".to_owned(),
                ..rel("a", "c")
            },
            // unresolved object (no id, no name) -> dropped
            KnowledgeRelationship {
                object: EntityRef {
                    id: None,
                    kind: None,
                    name: None,
                    aliases: Vec::new(),
                },
                ..rel("a", "b")
            },
        ];
        let edges = call_edges(&rels);
        assert_eq!(
            edges,
            vec![
                ("a".to_owned(), "b".to_owned()),
                ("c".to_owned(), "d".to_owned()),
            ]
        );
    }

    #[test]
    fn dead_code_returns_zero_caller_symbols() {
        // a -> b -> c -> d. Only `a` is never called.
        let rels = vec![rel("a", "b"), rel("b", "c"), rel("c", "d")];
        assert_eq!(dead_code(&rels), vec!["a".to_owned()]);
    }

    #[test]
    fn blast_radius_returns_transitive_callers() {
        // a -> b -> c -> d: callers of d within 5 hops are c, b, a.
        let rels = vec![rel("a", "b"), rel("b", "c"), rel("c", "d")];
        let radius = blast_radius(&rels, "d", 5);
        let expected: HashSet<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(radius, expected);
    }

    #[test]
    fn dependency_path_returns_shortest_call_path() {
        // a -> b -> c, plus a -> c: shortest a->c is the direct edge.
        let rels = vec![rel("a", "b"), rel("b", "c"), rel("a", "c")];
        assert_eq!(
            dependency_path(&rels, "a", "c"),
            Some(vec!["a".to_owned(), "c".to_owned()])
        );
    }

    #[test]
    fn dependency_path_none_when_unreachable() {
        let rels = vec![rel("a", "b"), rel("b", "c")];
        assert_eq!(dependency_path(&rels, "c", "a"), None);
    }

    #[test]
    fn central_symbols_ranks_hub_highest() {
        // a, b, c all call `hub` -> hub is the most central.
        let rels = vec![rel("a", "hub"), rel("b", "hub"), rel("c", "hub")];
        let central = central_symbols(&rels, 1);
        assert_eq!(central[0].0, "hub");
    }

    #[test]
    fn bridge_symbols_ranks_chokepoint_highest() {
        // a -> b -> c: b is the bridge.
        let rels = vec![rel("a", "b"), rel("b", "c")];
        let bridges = bridge_symbols(&rels, 1);
        assert_eq!(bridges[0].0, "b");
    }

    #[test]
    fn call_communities_collapses_tightly_coupled_symbols() {
        // A triangle is one community.
        let rels = vec![rel("a", "b"), rel("b", "c"), rel("a", "c")];
        let labels: HashSet<usize> = call_communities(&rels, 10).values().copied().collect();
        assert_eq!(labels.len(), 1);
    }

    #[test]
    fn symbol_context_returns_callers_callees_and_community() {
        // b is called by a; b calls c.
        let rels = vec![rel("a", "b"), rel("b", "c")];
        let ctx = symbol_context(&rels, "b", 5);
        assert_eq!(ctx.callers, vec!["a".to_owned()]);
        assert_eq!(ctx.callees, vec!["c".to_owned()]);
        assert!(ctx.community.is_some(), "b belongs to a community");
    }

    #[test]
    fn cyclomatic_complexity_counts_decision_points() {
        let simple = "fn add(a: i32, b: i32) -> i32 { a + b }";
        assert_eq!(cyclomatic_complexity(simple), 1);

        let branching = "fn check(x: i32) -> i32 {
            if x > 0 { return x; }
            for i in 0..x { println!(\"x\"); }
            while x > 0 { x -= 1; }
            x
        }";
        // 1 base + if + for + while = 4
        assert_eq!(cyclomatic_complexity(branching), 4);

        let logical = "fn both(a: bool, b: bool) -> bool { a && b || !a }";
        // 1 base + && + || = 3
        assert_eq!(cyclomatic_complexity(logical), 3);
    }

    #[test]
    fn find_endpoints_extracts_routes_from_source() {
        let src = r#"
            const app = express();
            app.get("/users", getUsers);
            app.post("/orders", createOrder);
            #[get("/health")]
            async fn health() -> &'static str { "ok" }
        "#;
        let endpoints = find_endpoints(src);
        assert_eq!(endpoints.len(), 3);
        assert!(endpoints.contains(&HttpEndpoint {
            method: "GET".to_owned(),
            path: "/users".to_owned()
        }));
        assert!(endpoints.contains(&HttpEndpoint {
            method: "POST".to_owned(),
            path: "/orders".to_owned()
        }));
        assert!(endpoints.contains(&HttpEndpoint {
            method: "GET".to_owned(),
            path: "/health".to_owned()
        }));
    }

    #[test]
    fn find_endpoints_rejects_false_positives() {
        let src = r#"
            forget("reasons")
            budget("hello")
            target("world")
        "#;
        assert!(find_endpoints(src).is_empty());
    }

    #[test]
    fn find_api_calls_extracts_http_targets() {
        let src = r#"
            const res = await fetch("/api/users");
            axios.post("/orders", payload);
            r = requests.get("https://api.example.com/health")
        "#;
        let calls = find_api_calls(src);
        assert_eq!(calls.len(), 3);
        assert!(calls.contains(&"/api/users".to_owned()));
        assert!(calls.contains(&"/orders".to_owned()));
        assert!(calls.contains(&"https://api.example.com/health".to_owned()));
    }

    #[test]
    fn find_api_calls_rejects_false_positives() {
        let src = r#"
            fetchData("/not-a-call")
            refetch("/also-not")
            prefetch("/nope")
        "#;
        assert!(find_api_calls(src).is_empty());
    }

    #[test]
    fn find_entry_points_detects_main_and_handlers() {
        let src = r#"
            fn main() { println!("hello"); }
            if __name__ == "__main__":
                main()
        "#;
        let entries = find_entry_points(src);
        assert!(entries.contains(&"main".to_owned()));
        assert!(entries.contains(&"__main__".to_owned()));
    }

    #[test]
    fn process_flow_traces_call_chain() {
        // a -> b -> c -> d: flow from a is [a, b, c, d].
        let rels = vec![rel("a", "b"), rel("b", "c"), rel("c", "d")];
        let flow = process_flow(&rels, "a", 5);
        assert_eq!(
            flow,
            vec![
                "a".to_owned(),
                "b".to_owned(),
                "c".to_owned(),
                "d".to_owned(),
            ]
        );
    }

    #[test]
    fn match_api_topology_links_calls_to_endpoints() {
        let endpoints = vec![
            HttpEndpoint {
                method: "GET".to_owned(),
                path: "/users".to_owned(),
            },
            HttpEndpoint {
                method: "POST".to_owned(),
                path: "/orders".to_owned(),
            },
        ];
        let calls = vec![
            "/users".to_owned(),
            "/users?page=1".to_owned(),
            "https://api.example.com/orders".to_owned(),
            "/health".to_owned(),
        ];
        let matches = match_api_topology(&endpoints, &calls);
        assert_eq!(matches.len(), 3);
        assert!(matches.contains(&("/users".to_owned(), "GET /users".to_owned())));
        assert!(matches.contains(&("/users?page=1".to_owned(), "GET /users".to_owned())));
        assert!(matches.contains(&(
            "https://api.example.com/orders".to_owned(),
            "POST /orders".to_owned()
        )));
    }

    #[test]
    fn resolve_call_targets_fills_name_only_refs() {
        let mut rels = vec![rel("caller", "external_fn")];
        rels[0].object.id = None;
        rels[0].object.name = Some("external_fn".to_owned());

        let mut name_map = std::collections::HashMap::new();
        name_map.insert("external_fn".to_owned(), "resolved-id".to_owned());

        let count = resolve_call_targets(&mut rels, &name_map);
        assert_eq!(count, 1);
        assert_eq!(rels[0].object.id.as_ref().unwrap().as_str(), "resolved-id");
    }

    #[test]
    fn most_complex_ranks_highest_first() {
        let sources = vec![
            (
                "simple".to_owned(),
                "fn add(a: i32, b: i32) -> i32 { a + b }".to_owned(),
            ),
            (
                "complex".to_owned(),
                "fn check(x: i32) -> i32 { if x > 0 { for i in 0..x { } } x }".to_owned(),
            ),
        ];
        let ranked = most_complex(&sources, 2);
        assert_eq!(ranked[0].0, "complex");
        assert!(ranked[0].1 > ranked[1].1);
    }

    #[test]
    fn repository_stats_counts_nodes_and_edges() {
        let rels = vec![rel("a", "b"), rel("b", "c"), rel("a", "c")];
        let stats = repository_stats(&rels);
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 3);
    }

    #[test]
    fn service_diagram_renders_mermaid() {
        let topology = vec![
            ("/api/users".to_owned(), "GET /users".to_owned()),
            ("/orders".to_owned(), "POST /orders".to_owned()),
        ];
        let diagram = service_diagram(&topology);
        assert!(diagram.starts_with("graph LR"));
        assert!(diagram.contains("\"/api/users\" --> \"GET /users\""));
        assert!(diagram.contains("\"/orders\" --> \"POST /orders\""));
    }

    #[test]
    fn symbol_context_traverses_protocol_edges() {
        // Protocol edges (sends_request, handled_by) should be traversable
        // alongside calls edges, so change_impact crosses the HTTP boundary.
        let rels = vec![
            rel_named("entity-a", "caller_fn", "entity-ep", "GET /api/foo"),
            rel_named("entity-ep", "GET /api/foo", "entity-b", "handler_fn"),
        ];
        // The first rel is sends_request, the second is handled_by.
        // But rel() hardcodes predicate="calls". Build manually.
        let protocol_rels = vec![
            KnowledgeRelationship {
                id: Id::from("rel-1"),
                graph_id: None,
                subject: ref_named("entity-a", "caller_fn"),
                predicate: "sends_request".to_owned(),
                object: ref_named("entity-ep", "GET /api/foo"),
                scope: Scope {
                    tenant: "t".to_owned(),
                    subject: None,
                    workspace: None,
                    session: None,
                    environment: None,
                },
                evidence: Vec::new(),
                confidence: None,
                provenance: provenance(),
                created_at: fixed_now(),
                updated_at: None,
            },
            KnowledgeRelationship {
                id: Id::from("rel-2"),
                graph_id: None,
                subject: ref_named("entity-ep", "GET /api/foo"),
                predicate: "handled_by".to_owned(),
                object: ref_named("entity-b", "handler_fn"),
                scope: Scope {
                    tenant: "t".to_owned(),
                    subject: None,
                    workspace: None,
                    session: None,
                    environment: None,
                },
                evidence: Vec::new(),
                confidence: None,
                provenance: provenance(),
                created_at: fixed_now(),
                updated_at: None,
            },
        ];
        let _ = rels; // unused — protocol_rels is the test data.
        let ctx = symbol_context(&protocol_rels, "caller_fn", 3);
        assert!(
            !ctx.callees.is_empty(),
            "callees should traverse sends_request: {ctx:?}"
        );
        assert!(
            ctx.callees.contains(&"GET /api/foo".to_owned()),
            "callees should include endpoint: {ctx:?}"
        );
        let ctx2 = symbol_context(&protocol_rels, "handler_fn", 3);
        assert!(
            !ctx2.callers.is_empty(),
            "callers should traverse handled_by: {ctx2:?}"
        );
    }

    #[test]
    fn symbol_context_matches_by_name_not_opaque_id() {
        // Regression: scanner entity IDs are opaque hashes (entity-…), but
        // symbol_context is called by function NAME. entity_key must return the
        // name so the query matches.
        let rels = vec![rel_named(
            "entity-hash-a",
            "caller_fn",
            "entity-hash-b",
            "callee_fn",
        )];
        let ctx = symbol_context(&rels, "caller_fn", 2);
        assert!(!ctx.callees.is_empty(), "callees by name: {ctx:?}");
        assert!(ctx.callees.contains(&"callee_fn".to_owned()));
    }

    #[test]
    fn symbol_context_bounded_not_truncated_under_cap() {
        // b called by a; b calls c. cap 64 -> no truncation.
        let rels = vec![rel("a", "b"), rel("b", "c")];
        let sc = symbol_context_bounded(&rels, "b", 5, 64);
        assert_eq!(sc.ctx.callers, vec!["a".to_owned()]);
        assert_eq!(sc.ctx.callees, vec!["c".to_owned()]);
        assert!(!sc.truncated, "under cap -> not truncated");
    }

    #[test]
    fn symbol_context_bounded_truncates_when_callees_exceed_cap() {
        // symbol -> n0 -> n1 -> ... -> n9 (10 callees). cap 5 -> callees capped, truncated.
        let mut rels = vec![rel("symbol", "n0")];
        for i in 0..9usize {
            rels.push(rel(
                format!("n{i}").as_str(),
                format!("n{}", i + 1).as_str(),
            ));
        }
        let sc = symbol_context_bounded(&rels, "symbol", 10, 5);
        assert_eq!(sc.ctx.callees.len(), 5, "callees capped at 5");
        assert!(sc.truncated, "callees (10) exceed cap (5) -> truncated");
        assert!(sc.ctx.callers.is_empty(), "symbol has no callers");
    }

    #[test]
    fn blast_radius_bounded_truncates_and_sorts_callers() {
        // target <- n0 <- n1 <- ... <- n9 (10 callers). cap 5 -> truncated, callers sorted.
        let mut rels = vec![rel("n0", "target")];
        for i in 1..=9usize {
            rels.push(rel(
                format!("n{i}").as_str(),
                format!("n{}", i - 1).as_str(),
            ));
        }
        let br = blast_radius_bounded(&rels, "target", 10, 5);
        assert_eq!(br.callers.len(), 5, "callers capped at 5");
        assert!(br.truncated, "callers (10) exceed cap (5) -> truncated");
        let mut sorted = br.callers.clone();
        sorted.sort();
        assert_eq!(br.callers, sorted, "callers sorted for determinism");
    }

    // --- fixtures ---

    fn rel(caller: &str, callee: &str) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: Id::from(format!("rel-{caller}-{callee}")),
            graph_id: None,
            subject: ref_of(caller),
            predicate: "calls".to_owned(),
            object: ref_of(callee),
            scope: Scope {
                tenant: "t".to_owned(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            evidence: Vec::new(),
            confidence: None,
            provenance: provenance(),
            created_at: fixed_now(),
            updated_at: None,
        }
    }

    fn ref_of(key: &str) -> EntityRef {
        EntityRef {
            id: Some(Id::from(key)),
            kind: None,
            name: None,
            aliases: Vec::new(),
        }
    }

    /// Like `ref_of` but sets BOTH an opaque id and a human-readable name (the
    /// scanner case: `id = entity-{hash}`, `name = function_name`).
    fn ref_named(id: &str, name: &str) -> EntityRef {
        EntityRef {
            id: Some(Id::from(id)),
            kind: None,
            name: Some(name.to_owned()),
            aliases: Vec::new(),
        }
    }

    /// A `calls` relationship using named refs (opaque id + name).
    fn rel_named(
        caller_id: &str,
        caller_name: &str,
        callee_id: &str,
        callee_name: &str,
    ) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: Id::from(format!("rel-{caller_name}-{callee_name}")),
            graph_id: None,
            subject: ref_named(caller_id, caller_name),
            predicate: "calls".to_owned(),
            object: ref_named(callee_id, callee_name),
            scope: Scope {
                tenant: "t".to_owned(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            evidence: Vec::new(),
            confidence: None,
            provenance: provenance(),
            created_at: fixed_now(),
            updated_at: None,
        }
    }

    fn provenance() -> Provenance {
        Provenance {
            source: "codegraph_queries_test".to_owned(),
            actor: Actor {
                id: Id::from("actor-test"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            observed_at: fixed_now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("test".to_owned()),
        }
    }

    fn fixed_now() -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2026, 7, 8, 12, 0, 0)
            .single()
            .expect("fixed timestamp")
    }
}
