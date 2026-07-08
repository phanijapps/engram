//! PageRank centrality over a directed edge list.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Computes PageRank scores for the nodes of a directed graph.
///
/// `edges` are `(source, target)` id pairs. Returns a map of node id -> score;
/// scores sum to approximately 1.0. `damping` is typically `0.85`; iteration
/// stops after `iterations` rounds or once the maximum per-node change falls
/// below `tol`. Nodes with no out-edges (dangling) distribute their mass
/// uniformly each round, so mass is conserved.
///
/// Generic over the node id type to keep the algorithm decoupled from domain
/// types; callers pass `String` (or any `Eq + Hash + Clone`) ids.
pub fn pagerank<N>(edges: &[(N, N)], damping: f64, iterations: usize, tol: f64) -> HashMap<N, f64>
where
    N: Eq + Hash + Clone,
{
    let mut nodes: HashSet<N> = HashSet::new();
    let mut out_neighbors: HashMap<N, Vec<N>> = HashMap::new();
    let mut in_neighbors: HashMap<N, Vec<N>> = HashMap::new();
    for (source, target) in edges {
        nodes.insert(source.clone());
        nodes.insert(target.clone());
        out_neighbors
            .entry(source.clone())
            .or_default()
            .push(target.clone());
        in_neighbors
            .entry(target.clone())
            .or_default()
            .push(source.clone());
    }

    let n = nodes.len();
    if n == 0 {
        return HashMap::new();
    }

    let dangling: Vec<N> = nodes
        .iter()
        .filter(|id| !out_neighbors.contains_key(*id))
        .cloned()
        .collect();

    let mut scores: HashMap<N, f64> = nodes
        .iter()
        .map(|id| (id.clone(), 1.0 / n as f64))
        .collect();

    for _ in 0..iterations {
        let mut next: HashMap<N, f64> = nodes
            .iter()
            .map(|id| (id.clone(), (1.0 - damping) / n as f64))
            .collect();

        // Distribute dangling-node mass uniformly so PageRank is conserved.
        let dangling_mass: f64 =
            dangling.iter().map(|id| scores[id]).sum::<f64>() * damping / n as f64;
        for id in nodes.iter() {
            *next.get_mut(id).unwrap() += dangling_mass;
        }

        // Incoming contributions: each node gains damping * sum(pr[m] / outdeg(m)).
        for id in nodes.iter() {
            let incoming = in_neighbors
                .get(id)
                .map(|neighbors| {
                    neighbors
                        .iter()
                        .map(|m| scores[m] / out_neighbors[m].len() as f64)
                        .sum::<f64>()
                })
                .unwrap_or(0.0);
            *next.get_mut(id).unwrap() += damping * incoming;
        }

        let max_delta = nodes
            .iter()
            .map(|id| (next[id] - scores[id]).abs())
            .fold(0.0_f64, f64::max);
        scores = next;
        if max_delta < tol {
            break;
        }
    }

    scores
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|&(s, t)| (s.to_owned(), t.to_owned()))
            .collect()
    }

    #[test]
    fn cycle_is_near_uniform() {
        let e = edges(&[("a", "b"), ("b", "c"), ("c", "a")]);
        let pr = pagerank(&e, 0.85, 100, 1e-6);
        for id in ["a", "b", "c"] {
            let score = pr[id];
            assert!(
                (score - 1.0 / 3.0).abs() < 1e-3,
                "{id} score {score} should be near 1/3"
            );
        }
    }

    #[test]
    fn sink_ranks_strictly_highest() {
        // Two sources -> one sink. The sink has inbound from both; sources have none.
        let e = edges(&[("a", "c"), ("b", "c")]);
        let pr = pagerank(&e, 0.85, 100, 1e-6);
        assert!(pr["c"] > pr["a"], "sink must outrank source a");
        assert!(pr["c"] > pr["b"], "sink must outrank source b");
    }

    #[test]
    fn scores_sum_to_approximately_one() {
        let e = edges(&[("a", "b"), ("a", "c"), ("b", "c"), ("c", "b")]);
        let pr = pagerank(&e, 0.85, 100, 1e-6);
        let sum: f64 = pr.values().sum();
        assert!(
            (sum - 1.0).abs() < 1e-6,
            "scores sum to {sum}, expected ~1.0"
        );
    }

    #[test]
    fn empty_graph_returns_empty() {
        let pr = pagerank::<String>(&[], 0.85, 100, 1e-6);
        assert!(pr.is_empty());
    }

    #[test]
    fn deterministic_across_runs() {
        let e = edges(&[("a", "b"), ("b", "c"), ("c", "a"), ("a", "c")]);
        let first = pagerank(&e, 0.85, 50, 1e-8);
        let second = pagerank(&e, 0.85, 50, 1e-8);
        assert_eq!(first, second);
    }
}
