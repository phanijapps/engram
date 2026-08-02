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

/// Like [`ancestors`], but bounds the result to at most `max_visited` nodes and
/// reports whether the bound prevented further exploration. The visited cap is a
/// safety net for raised-depth or super-hub queries; `max_depth` remains the
/// outer bound. The cap bounds the **ancestors** direction independently.
///
/// `truncated` is `true` iff the BFS encountered a *newly*-reachable node while
/// already at capacity (i.e. the natural reachable set strictly exceeds
/// `max_visited`). When the natural reachable set is at or under the cap, the
/// queue drains and `truncated` is `false` — including the exact-boundary case
/// where the set equals `max_visited`. Duplicate enqueues never count as
/// truncation.
pub fn ancestors_bounded<N>(
    edges: &[(N, N)],
    target: &N,
    max_depth: usize,
    max_visited: usize,
) -> (HashSet<N>, bool)
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

    let mut truncated = false;
    while let Some((node, depth)) = queue.pop_front() {
        if found.contains(&node) {
            continue; // already visited (duplicate enqueue)
        }
        // `node` is newly reachable and not yet visited.
        if found.len() >= max_visited {
            // At capacity, yet a new node is reachable -> the cap is preventing
            // exploration. Signal truncation and stop without visiting it.
            truncated = true;
            break;
        }
        found.insert(node.clone());
        if depth < max_depth {
            if let Some(preds) = predecessors.get(&node) {
                for pred in preds {
                    queue.push_back((pred.clone(), depth + 1));
                }
            }
        }
    }
    (found, truncated)
}

/// Like [`descendants`], but bounds the result to at most `max_visited` nodes and
/// reports whether the bound prevented exploration. The visited cap bounds the
/// **descendants** direction independently of ancestors. See [`ancestors_bounded`]
/// for the truncation semantics.
pub fn descendants_bounded<N>(
    edges: &[(N, N)],
    source: &N,
    max_depth: usize,
    max_visited: usize,
) -> (HashSet<N>, bool)
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

    let mut truncated = false;
    while let Some((node, depth)) = queue.pop_front() {
        if found.contains(&node) {
            continue;
        }
        if found.len() >= max_visited {
            truncated = true;
            break;
        }
        found.insert(node.clone());
        if depth < max_depth {
            if let Some(children) = adjacency.get(&node) {
                for child in children {
                    queue.push_back((child.clone(), depth + 1));
                }
            }
        }
    }
    (found, truncated)
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

    // --- bounded variants --------------------------------------------------

    #[test]
    fn ancestors_bounded_returns_full_set_under_cap() {
        // a -> b -> c -> d: ancestors of d (depth 5, cap 64) = {a,b,c}, under cap.
        let e = edges(&[("a", "b"), ("b", "c"), ("c", "d")]);
        let (set, truncated) = ancestors_bounded(&e, &"d".to_string(), 5, 64);
        let expected: HashSet<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(set, expected);
        assert!(!truncated, "reachable set under cap -> not truncated");
    }

    #[test]
    fn ancestors_bounded_caps_and_signals_truncation_when_exceeded() {
        // Linear chain n0..n70 -> target: 71 ancestors. cap 64 -> 64 visited, truncated.
        let mut pairs: Vec<(String, String)> = Vec::new();
        for i in 0..=70usize {
            let from = format!("n{i}");
            let to = if i < 70 {
                format!("n{}", i + 1)
            } else {
                "target".to_string()
            };
            pairs.push((from, to));
        }
        let (set, truncated) = ancestors_bounded(&pairs, &"target".to_string(), 100, 64);
        assert_eq!(set.len(), 64, "capped at 64");
        assert!(truncated, "reachable set (71) exceeds cap -> truncated");
    }

    #[test]
    fn ancestors_bounded_exact_boundary_is_not_truncated() {
        // Exactly 64 ancestors, cap 64 -> all fit, queue drains naturally, NOT truncated.
        let mut pairs: Vec<(String, String)> = Vec::new();
        for i in 0..=63usize {
            let from = format!("n{i}");
            let to = if i < 63 {
                format!("n{}", i + 1)
            } else {
                "target".to_string()
            };
            pairs.push((from, to));
        }
        let (set, truncated) = ancestors_bounded(&pairs, &"target".to_string(), 100, 64);
        assert_eq!(set.len(), 64, "all 64 ancestors fit exactly");
        assert!(!truncated, "natural set == cap exactly -> not truncated");
    }

    #[test]
    fn ancestors_bounded_respects_depth() {
        // a -> b -> c -> d -> e: ancestors of e, depth 2, cap 64 = {d, c}.
        let e = edges(&[("a", "b"), ("b", "c"), ("c", "d"), ("d", "e")]);
        let (set, truncated) = ancestors_bounded(&e, &"e".to_string(), 2, 64);
        let expected: HashSet<String> = ["d", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(set, expected);
        assert!(!truncated);
    }

    #[test]
    fn descendants_bounded_caps_and_signals_truncation_when_exceeded() {
        // target -> n0 -> n1 -> ... -> n70: 71 descendants. cap 64 -> truncated.
        let mut pairs: Vec<(String, String)> = Vec::new();
        for i in 0..=70usize {
            let from = if i == 0 {
                "target".to_string()
            } else {
                format!("n{}", i - 1)
            };
            let to = format!("n{i}");
            pairs.push((from, to));
        }
        let (set, truncated) = descendants_bounded(&pairs, &"target".to_string(), 100, 64);
        assert_eq!(set.len(), 64, "capped at 64");
        assert!(truncated, "reachable set (71) exceeds cap -> truncated");
    }

    #[test]
    fn descendants_bounded_exact_boundary_is_not_truncated() {
        // Exactly 64 descendants, cap 64 -> all fit, NOT truncated.
        let mut pairs: Vec<(String, String)> = Vec::new();
        for i in 0..=63usize {
            let from = if i == 0 {
                "target".to_string()
            } else {
                format!("n{}", i - 1)
            };
            let to = format!("n{i}");
            pairs.push((from, to));
        }
        let (set, truncated) = descendants_bounded(&pairs, &"target".to_string(), 100, 64);
        assert_eq!(set.len(), 64, "all 64 descendants fit exactly");
        assert!(!truncated, "natural set == cap exactly -> not truncated");
    }

    #[test]
    fn ancestors_bounded_duplicate_enqueue_does_not_false_truncate() {
        // Diamond: a -> b, a -> c, b -> d, c -> d. Ancestors of d = {b, c, a} (a via two paths).
        // cap 3 -> all 3 fit; a's duplicate enqueue must NOT trip truncation -> truncated == false.
        let e = edges(&[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")]);
        let (set, truncated) = ancestors_bounded(&e, &"d".to_string(), 5, 3);
        let expected: HashSet<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(set, expected);
        assert!(
            !truncated,
            "duplicate enqueue of 'a' must not false-truncate"
        );
    }
}
