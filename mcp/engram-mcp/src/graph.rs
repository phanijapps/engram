//! General knowledge-graph traversal tools (RFC-0016 P1, Layer 2).
//!
//! Operate over the UNIFIED graph — any entity kind (concept, function, class,
//! …) — so the doc↔code edges created by the P0 `describes` bridge are
//! explorable. Read-only: built on `KnowledgeQuery` reads (`list_entities` /
//! `list_relationships`); no new provider handle, no engine-store bypass. Unlike
//! the `codegraph` composites these are not code-specific — a concept, an API,
//! or a function are all first-class nodes.

use std::collections::{HashMap, HashSet};

use engram_domain::KnowledgeEntity;
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::codegraph::fetch_rels;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::req_str;

/// Fetch all entities in the project scope (returns empty vec on error).
fn fetch_entities(app: &App) -> Vec<KnowledgeEntity> {
    app.provider
        .require_knowledge_query()
        .ok()
        .and_then(|q| block_on(q.list_entities(&app.scope)).ok())
        .unwrap_or_default()
}

/// `graph_neighbors`: every entity directly connected to `name` and the edges
/// between them (both directions). Works on any entity kind — e.g. a concept
/// `describes` a function, or a function `calls` another.
pub fn graph_neighbors(app: &App, args: &Value) -> Result<Value, ToolError> {
    let name = req_str(args, "name")?;
    let limit = args["limit"]
        .as_u64()
        .map(|n| n as usize)
        .unwrap_or(100)
        .clamp(1, 1000);
    let rels = fetch_rels(app);
    let mut edges: Vec<String> = Vec::new();
    for r in &rels {
        let (Some(s), Some(o)) = (r.subject.name.as_deref(), r.object.name.as_deref()) else {
            continue;
        };
        if s == name {
            edges.push(format!("{name} -[{}]-> {o}", r.predicate));
        } else if o == name {
            edges.push(format!("{s} -[{}]-> {name}", r.predicate));
        }
        if edges.len() >= limit {
            break;
        }
    }
    let body = if edges.is_empty() {
        format!("No neighbors found for '{name}'.")
    } else {
        format!(
            "Neighbors of '{name}' ({} edge(s)):\n{}",
            edges.len(),
            edges.join("\n")
        )
    };
    Ok(protocol::text_content(body))
}

/// `graph_subgraph`: a breadth-first subgraph around `name` up to `depth` hops.
/// Explores both directions but labels each edge with its natural direction, so
/// the doc↔code connections (and call chains) read correctly from any start node.
pub fn graph_subgraph(app: &App, args: &Value) -> Result<Value, ToolError> {
    let name = req_str(args, "name")?;
    let depth = args["depth"].as_u64().unwrap_or(2) as usize;
    let limit = args["limit"]
        .as_u64()
        .map(|n| n as usize)
        .unwrap_or(100)
        .clamp(1, 1000);
    let rels = fetch_rels(app);

    // Adjacency: node -> [(neighbor, predicate, node_is_subject)]. Both
    // directions are added so BFS can traverse into a node via an incoming edge,
    // but the emitted label keeps the edge's natural direction.
    let mut adj: HashMap<String, Vec<(String, String, bool)>> = HashMap::new();
    for r in &rels {
        let (Some(s), Some(o)) = (r.subject.name.as_deref(), r.object.name.as_deref()) else {
            continue;
        };
        adj.entry(s.to_owned())
            .or_default()
            .push((o.to_owned(), r.predicate.clone(), true));
        adj.entry(o.to_owned())
            .or_default()
            .push((s.to_owned(), r.predicate.clone(), false));
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut emitted: HashSet<String> = HashSet::new();
    let mut edges: Vec<String> = Vec::new();
    let mut frontier: Vec<String> = vec![name.to_owned()];
    for _ in 0..=depth {
        if frontier.is_empty() || edges.len() >= limit {
            break;
        }
        let mut next: Vec<String> = Vec::new();
        for node in &frontier {
            if !visited.insert(node.clone()) {
                continue;
            }
            let Some(neighbors) = adj.get(node) else {
                continue;
            };
            for (nbr, pred, node_is_subject) in neighbors {
                let label = if *node_is_subject {
                    format!("{node} -[{pred}]-> {nbr}")
                } else {
                    format!("{nbr} -[{pred}]-> {node}")
                };
                if emitted.insert(label.clone()) {
                    edges.push(label);
                    if edges.len() >= limit {
                        break;
                    }
                }
                if !visited.contains(nbr) {
                    next.push(nbr.clone());
                }
            }
            if edges.len() >= limit {
                break;
            }
        }
        frontier = next;
    }

    let body = if edges.is_empty() {
        format!("No subgraph around '{name}' (entity not found or no edges).")
    } else {
        format!(
            "Subgraph around '{name}' (depth {depth}, {} edge(s)):\n{}",
            edges.len(),
            edges.join("\n")
        )
    };
    Ok(protocol::text_content(body))
}

/// `resolve_entity`: resolve a name to its entity (exact match, else first
/// substring) with kind, id, graph, source-ref count, and aliases. The
/// "is X in the graph?" lookup.
pub fn resolve_entity(app: &App, args: &Value) -> Result<Value, ToolError> {
    let name = req_str(args, "name")?;
    let entities = fetch_entities(app);
    let needle = name.to_lowercase();
    let entity = entities.iter().find(|e| e.name == name).or_else(|| {
        entities
            .iter()
            .find(|e| e.name.to_lowercase().contains(&needle))
    });
    let body = match entity {
        Some(e) => {
            let graph = e
                .graph_id
                .as_ref()
                .map(|g| g.to_string())
                .unwrap_or_else(|| "(none)".to_owned());
            format!(
                "Resolved '{name}' -> {} ({:?})\nid: {}\ngraph: {}\nsource_refs: {}\naliases: {:?}",
                e.name,
                e.kind,
                e.id,
                graph,
                e.source_refs.len(),
                e.aliases
            )
        }
        None => format!("No entity found for '{name}'."),
    };
    Ok(protocol::text_content(body))
}
