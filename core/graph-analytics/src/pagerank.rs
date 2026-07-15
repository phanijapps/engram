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

/// Computes Personalized PageRank scores seeded at `seeds`.
///
/// Like [`pagerank`], but the teleport mass is concentrated on `seeds` (each
/// seed receives `1/|seeds|`; non-seeds receive `0`) instead of distributed
/// uniformly, and dangling-node mass is redistributed through that same
/// personalization vector rather than uniformly. The result ranks nodes by graph
/// proximity to the seeds: seeds and their neighborhoods score highest. Scores
/// sum to approximately 1.0.
///
/// `edges` are `(source, target)` id pairs; `seeds` are the node ids to
/// personalize on. Returns an empty map when `seeds` is empty (no
/// personalization) or when the resulting node set is empty. Seeds that appear
/// in no edge are still added to the node set, so an isolated seed receives its
/// teleport mass and appears in the output (its score tends to 1.0 when it has
/// no edges).
///
/// Generic over the node id type; callers pass `String` ids. No dependencies.
pub fn personalized_pagerank<N>(
    edges: &[(N, N)],
    seeds: &[N],
    damping: f64,
    iterations: usize,
    tol: f64,
) -> HashMap<N, f64>
where
    N: Eq + Hash + Clone,
{
    if seeds.is_empty() {
        return HashMap::new();
    }

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

    // Seeds are part of the node set even when they have no edges.
    let seed_set: HashSet<N> = seeds.iter().cloned().collect();
    for seed in &seed_set {
        nodes.insert(seed.clone());
    }

    if nodes.is_empty() {
        return HashMap::new();
    }
    let p = 1.0 / seed_set.len() as f64;

    let dangling: Vec<N> = nodes
        .iter()
        .filter(|id| !out_neighbors.contains_key(*id))
        .cloned()
        .collect();

    let n = nodes.len() as f64;
    let mut scores: HashMap<N, f64> = nodes.iter().map(|id| (id.clone(), 1.0 / n)).collect();

    for _ in 0..iterations {
        // Teleport mass is concentrated on seeds: (1 - damping) * p per seed, 0
        // otherwise.
        let mut next: HashMap<N, f64> = nodes
            .iter()
            .map(|id| {
                let teleport = if seed_set.contains(id) {
                    (1.0 - damping) * p
                } else {
                    0.0
                };
                (id.clone(), teleport)
            })
            .collect();

        // Dangling-node mass redistributes through the personalization vector
        // (to seeds only), conserving mass while keeping it near the seeds.
        let dangling_mass: f64 = dangling.iter().map(|id| scores[id]).sum::<f64>() * damping * p;
        for id in nodes.iter() {
            if seed_set.contains(id) {
                *next.get_mut(id).unwrap() += dangling_mass;
            }
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

    #[test]
    fn ppr_seeds_and_neighbors_outrank_distant_nodes() {
        // Directed chain a -> b -> c -> d, personalized at a. Proximity to the
        // seed must decay monotonically down the chain.
        let e = edges(&[("a", "b"), ("b", "c"), ("c", "d")]);
        let seeds: Vec<String> = vec!["a".to_string()];
        let pr = personalized_pagerank(&e, &seeds, 0.85, 500, 1e-12);
        assert!(pr["a"] > pr["b"], "seed a must outrank neighbor b");
        assert!(pr["b"] > pr["c"], "b must outrank c");
        assert!(pr["c"] > pr["d"], "c must outrank distant d");
    }

    #[test]
    fn ppr_scores_sum_to_approximately_one() {
        let e = edges(&[("a", "b"), ("b", "c"), ("c", "a")]);
        let seeds: Vec<String> = vec!["a".to_string()];
        let pr = personalized_pagerank(&e, &seeds, 0.85, 500, 1e-12);
        let sum: f64 = pr.values().sum();
        assert!(
            (sum - 1.0).abs() < 1e-6,
            "scores sum to {sum}, expected ~1.0"
        );
    }

    #[test]
    fn ppr_empty_seeds_returns_empty() {
        let e = edges(&[("a", "b")]);
        let seeds: Vec<String> = vec![];
        let pr = personalized_pagerank(&e, &seeds, 0.85, 100, 1e-6);
        assert!(pr.is_empty());
    }

    #[test]
    fn ppr_isolated_seed_holds_all_mass() {
        // No edges at all: the seed is its own dangling node; with the
        // personalization vector concentrated on it, all mass stays on it.
        let seeds: Vec<String> = vec!["z".to_string()];
        let pr = personalized_pagerank::<String>(&[], &seeds, 0.85, 200, 1e-12);
        assert_eq!(pr.len(), 1);
        assert!(
            (pr["z"] - 1.0).abs() < 1e-6,
            "isolated seed z should hold all mass, got {}",
            pr["z"]
        );
    }

    #[test]
    fn ppr_isolated_seed_appears_and_does_not_leak() {
        // Seed a has no edges; b -> c is a separate component. Dangling mass
        // (including a's) redistributes through the personalization vector, so it
        // stays on a rather than leaking uniformly to b and c.
        let e = edges(&[("b", "c")]);
        let seeds: Vec<String> = vec!["a".to_string()];
        let pr = personalized_pagerank(&e, &seeds, 0.85, 500, 1e-12);
        assert!(
            pr.contains_key("a"),
            "isolated seed a must appear in output"
        );
        assert!(
            pr["a"] > pr["b"],
            "mass should stay on seed a, not leak to b"
        );
        assert!(
            pr["a"] > pr["c"],
            "mass should stay on seed a, not leak to c"
        );
    }

    #[test]
    fn ppr_converges_to_known_fixture() {
        // a -> b, a -> c, b -> c ; seed a. c (two inbound) outranks b (one
        // inbound) even though both are direct/indirect neighbors of a.
        let e = edges(&[("a", "b"), ("a", "c"), ("b", "c")]);
        let seeds: Vec<String> = vec!["a".to_string()];
        let pr = personalized_pagerank(&e, &seeds, 0.85, 1000, 1e-13);
        assert!(pr["a"] > pr["c"], "seed a must rank highest");
        assert!(
            pr["c"] > pr["b"],
            "c (two inbound) must outrank b (one inbound)"
        );
        let sum: f64 = pr.values().sum();
        assert!(
            (sum - 1.0).abs() < 1e-6,
            "scores sum to {sum}, expected ~1.0"
        );
    }

    #[test]
    fn ppr_deterministic_across_runs() {
        let e = edges(&[("a", "b"), ("b", "c"), ("c", "a"), ("a", "c")]);
        let seeds: Vec<String> = vec!["a".to_string()];
        let first = personalized_pagerank(&e, &seeds, 0.85, 200, 1e-10);
        let second = personalized_pagerank(&e, &seeds, 0.85, 200, 1e-10);
        assert_eq!(first, second);
    }
}
