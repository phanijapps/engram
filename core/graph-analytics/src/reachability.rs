//! Graph traversal primitives: degree, reverse-reachability, shortest path.
//!
//! These underpin the code-specific queries dead-code (zero in-degree on `calls`),
//! blast-radius (transitive callers via reverse reachability), and dependency-path
//! (shortest path along call edges). Generic over the node id type and decoupled
//! from domain types, like the rest of this crate.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

/// Returns the in-degree (number of incoming edges) per node.
///
/// Nodes with no incoming edges are absent from the map; treat missing as 0.
/// `dead_code` is the in-degree-zero set over `calls` edges.
pub fn in_degree<N>(edges: &[(N, N)]) -> HashMap<N, usize>
where
    N: Eq + Hash + Clone,
{
    let mut degree: HashMap<N, usize> = HashMap::new();
    for (_, target) in edges {
        *degree.entry(target.clone()).or_insert(0) += 1;
    }
    degree
}

/// Returns the set of nodes that can reach `target` within `max_depth` hops
/// (reverse BFS over `edges`). The transitive callers of `target` — its
/// **blast radius**. Excludes `target` itself.
pub fn ancestors<N>(edges: &[(N, N)], target: &N, max_depth: usize) -> HashSet<N>
where
    N: Eq + Hash + Clone,
{
    let mut predecessors: HashMap<N, Vec<N>> = HashMap::new();
    for (source, dest) in edges {
        predecessors
            .entry(dest.clone())
            .or_default()
            .push(source.clone());
    }

    let mut found: HashSet<N> = HashSet::new();
    let mut queue: VecDeque<(N, usize)> = VecDeque::new();
    if let Some(direct) = predecessors.get(target) {
        for pred in direct {
            queue.push_back((pred.clone(), 1));
        }
    }

    while let Some((node, depth)) = queue.pop_front() {
        if !found.insert(node.clone()) {
            continue;
        }
        if depth >= max_depth {
            continue;
        }
        if let Some(preds) = predecessors.get(&node) {
            for pred in preds {
                queue.push_back((pred.clone(), depth + 1));
            }
        }
    }
    found
}

/// Returns the set of nodes reachable from `source` within `max_depth` hops
/// (forward BFS over `edges`). The transitive callees of `source`. Excludes
/// `source` itself.
pub fn descendants<N>(edges: &[(N, N)], source: &N, max_depth: usize) -> HashSet<N>
where
    N: Eq + Hash + Clone,
{
    let mut adjacency: HashMap<N, Vec<N>> = HashMap::new();
    for (from, to) in edges {
        adjacency.entry(from.clone()).or_default().push(to.clone());
    }

    let mut found: HashSet<N> = HashSet::new();
    let mut queue: VecDeque<(N, usize)> = VecDeque::new();
    if let Some(direct) = adjacency.get(source) {
        for child in direct {
            queue.push_back((child.clone(), 1));
        }
    }

    while let Some((node, depth)) = queue.pop_front() {
        if !found.insert(node.clone()) {
            continue;
        }
        if depth >= max_depth {
            continue;
        }
        if let Some(children) = adjacency.get(&node) {
            for child in children {
                queue.push_back((child.clone(), depth + 1));
            }
        }
    }
    found
}

/// Returns the shortest path `from -> to` along `edges` (BFS, inclusive
/// endpoints), or `None` if `to` is unreachable. The **dependency path**.
pub fn shortest_path<N>(edges: &[(N, N)], from: &N, to: &N) -> Option<Vec<N>>
where
    N: Eq + Hash + Clone,
{
    let mut adjacency: HashMap<N, Vec<N>> = HashMap::new();
    for (source, dest) in edges {
        adjacency
            .entry(source.clone())
            .or_default()
            .push(dest.clone());
    }

    let mut visited: HashSet<N> = HashSet::new();
    let mut queue: VecDeque<Vec<N>> = VecDeque::new();
    visited.insert(from.clone());
    queue.push_back(vec![from.clone()]);

    while let Some(path) = queue.pop_front() {
        let last = path.last().expect("path is non-empty");
        if last == to {
            return Some(path);
        }
        if let Some(neighbors) = adjacency.get(last) {
            for next in neighbors {
                if visited.insert(next.clone()) {
                    let mut extended = path.clone();
                    extended.push(next.clone());
                    queue.push_back(extended);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|&(s, t)| (s.to_string(), t.to_string()))
            .collect()
    }

    #[test]
    fn in_degree_counts_incoming() {
        // a -> b -> c, plus a -> c.
        let e = edges(&[("a", "b"), ("b", "c"), ("a", "c")]);
        let deg = in_degree(&e);
        assert_eq!(deg.get("a"), None); // 0 incoming
        assert_eq!(deg["b"], 1);
        assert_eq!(deg["c"], 2);
    }

    #[test]
    fn ancestors_returns_transitive_callers() {
        // a -> b -> c -> d: callers of d (within 5 hops) are c, b, a.
        let e = edges(&[("a", "b"), ("b", "c"), ("c", "d")]);
        let callers = ancestors(&e, &"d".to_string(), 5);
        let mut expected: HashSet<String> = HashSet::new();
        for id in ["a", "b", "c"] {
            expected.insert(id.to_string());
        }
        assert_eq!(callers, expected);
        assert!(!callers.contains("d"), "target is not its own ancestor");
    }

    #[test]
    fn ancestors_respects_max_depth() {
        let e = edges(&[("a", "b"), ("b", "c"), ("c", "d")]);
        // depth 1: only the direct caller of d (c).
        let callers = ancestors(&e, &"d".to_string(), 1);
        assert_eq!(callers.len(), 1);
        assert!(callers.contains("c"));
    }

    #[test]
    fn descendants_returns_transitive_callees() {
        // a -> b -> c -> d: callees of a within 5 hops are b, c, d.
        let e = edges(&[("a", "b"), ("b", "c"), ("c", "d")]);
        let callees = descendants(&e, &"a".to_string(), 5);
        let mut expected: HashSet<String> = HashSet::new();
        for id in ["b", "c", "d"] {
            expected.insert(id.to_string());
        }
        assert_eq!(callees, expected);
        assert!(!callees.contains("a"), "source is not its own descendant");
    }

    #[test]
    fn descendants_respects_max_depth() {
        let e = edges(&[("a", "b"), ("b", "c"), ("c", "d")]);
        // depth 1: only the direct callee of a (b).
        let callees = descendants(&e, &"a".to_string(), 1);
        assert_eq!(callees.len(), 1);
        assert!(callees.contains("b"));
    }

    #[test]
    fn shortest_path_picks_direct_edge() {
        // a -> b -> c, plus a -> c: shortest a->c is the direct edge.
        let e = edges(&[("a", "b"), ("b", "c"), ("a", "c")]);
        let path = shortest_path(&e, &"a".to_string(), &"c".to_string());
        assert_eq!(path, Some(vec!["a".to_string(), "c".to_string()]));
    }

    #[test]
    fn shortest_path_none_when_unreachable() {
        let e = edges(&[("a", "b"), ("b", "c")]);
        assert_eq!(shortest_path(&e, &"c".to_string(), &"a".to_string()), None);
    }

    #[test]
    fn shortest_path_from_equals_to() {
        let e = edges(&[("a", "b")]);
        assert_eq!(
            shortest_path(&e, &"a".to_string(), &"a".to_string()),
            Some(vec!["a".to_string()])
        );
    }
}
