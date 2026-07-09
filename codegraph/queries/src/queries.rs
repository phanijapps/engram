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
