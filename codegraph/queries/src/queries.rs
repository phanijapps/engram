//! Map knowledge-graph call edges to a generic edge list and answer
//! code-specific queries over them.

use std::collections::{HashMap, HashSet};

use engram_domain::{EntityRef, KnowledgeRelationship};

/// Stable string key for an entity reference: its resolved id, else its name.
/// Returns `None` for a ref with neither (it cannot participate in a query).
pub fn entity_key(reference: &EntityRef) -> Option<String> {
    if let Some(id) = &reference.id {
        return Some(id.as_str().to_owned());
    }
    reference.name.clone()
}

/// Extracts `(caller, callee)` string pairs from `calls` relationships.
/// Other predicates and refs without a key are skipped.
pub fn call_edges(relationships: &[KnowledgeRelationship]) -> Vec<(String, String)> {
    relationships
        .iter()
        .filter(|r| r.predicate == "calls")
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
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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
