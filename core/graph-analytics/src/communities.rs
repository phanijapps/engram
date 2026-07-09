//! Community detection via single-level modularity-greedy Louvain local moving.

use std::collections::HashMap;
use std::hash::Hash;

/// Detects communities by greedily maximizing modularity.
///
/// Implements the **local-moving phase** of Louvain: each node starts in its own
/// community and repeatedly moves to the neighbor community that maximizes the
/// modularity gain `k_i_in(C) - Σ_tot(C)·k_i / 2m`, until a full pass moves
/// nothing or `max_passes` is reached. `edges` are treated as an undirected
/// weighted graph (each input edge weight 1; self-loops skipped).
///
/// Returns a map of node id -> community label (`0..k`). Deterministic: fixed
/// node order, lower community id wins ties.
///
/// The multi-level aggregation phase (collapse communities into supernodes and
/// re-run) is a follow-up; it yields tighter communities on large graphs but the
/// single-level phase already finds the natural partition on most fixtures.
pub fn communities<N>(edges: &[(N, N)], max_passes: usize) -> HashMap<N, usize>
where
    N: Eq + Hash + Clone,
{
    let mut index_of: HashMap<N, usize> = HashMap::new();
    let mut nodes: Vec<N> = Vec::new();
    let mut adj: Vec<HashMap<usize, f64>> = Vec::new();
    let mut m = 0.0_f64;
    for (source, target) in edges {
        if source == target {
            continue;
        }
        let si = ensure(source, &mut index_of, &mut nodes, &mut adj);
        let ti = ensure(target, &mut index_of, &mut nodes, &mut adj);
        *adj[si].entry(ti).or_insert(0.0) += 1.0;
        *adj[ti].entry(si).or_insert(0.0) += 1.0;
        m += 1.0;
    }

    let n = nodes.len();
    if n == 0 {
        return HashMap::new();
    }
    let two_m = 2.0 * m;
    if two_m == 0.0 {
        // No edges: every node is its own community. (Unreachable in practice —
        // nodes are only discovered via edges — but guards the divide-by-zero.)
        return nodes
            .iter()
            .enumerate()
            .map(|(i, v)| (v.clone(), i))
            .collect();
    }

    let k: Vec<f64> = (0..n).map(|i| adj[i].values().sum()).collect();
    let mut comm: Vec<usize> = (0..n).collect();
    let mut sigma_tot: Vec<f64> = k.clone();

    let mut improved = true;
    let mut passes = 0;
    while improved && passes < max_passes {
        improved = false;
        passes += 1;
        for i in 0..n {
            let ci = comm[i];
            let ki = k[i];
            sigma_tot[ci] -= ki;

            // Weighted edges from i into each neighboring community.
            let mut ki_in: HashMap<usize, f64> = HashMap::new();
            for (&j, &w) in &adj[i] {
                *ki_in.entry(comm[j]).or_insert(0.0) += w;
            }

            let mut best = ci;
            let mut best_gain = ki_in.get(&ci).copied().unwrap_or(0.0) - sigma_tot[ci] * ki / two_m;
            for (&c, &w) in &ki_in {
                if c == ci {
                    continue;
                }
                let gain = w - sigma_tot[c] * ki / two_m;
                let strictly_better = gain > best_gain + 1e-12;
                let tie_lower = (gain - best_gain).abs() <= 1e-12 && c < best;
                if strictly_better || tie_lower {
                    best = c;
                    best_gain = gain;
                }
            }

            comm[i] = best;
            sigma_tot[best] += ki;
            if best != ci {
                improved = true;
            }
        }
    }

    // Renumber communities to a dense 0..k label range.
    let mut labels: HashMap<usize, usize> = HashMap::new();
    let mut next_label = 0;
    let mut out = HashMap::with_capacity(n);
    for (i, node) in nodes.iter().enumerate() {
        let label = *labels.entry(comm[i]).or_insert_with(|| {
            let l = next_label;
            next_label += 1;
            l
        });
        out.insert(node.clone(), label);
    }
    out
}

fn ensure<N>(
    node: &N,
    index_of: &mut HashMap<N, usize>,
    nodes: &mut Vec<N>,
    adj: &mut Vec<HashMap<usize, f64>>,
) -> usize
where
    N: Eq + Hash + Clone,
{
    if let Some(&i) = index_of.get(node) {
        return i;
    }
    let i = nodes.len();
    nodes.push(node.clone());
    adj.push(HashMap::new());
    index_of.insert(node.clone(), i);
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn edges(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|&(s, t)| (s.to_string(), t.to_string()))
            .collect()
    }

    #[test]
    fn triangle_collapses_to_one_community() {
        let e = edges(&[("a", "b"), ("b", "c"), ("a", "c")]);
        let c = communities(&e, 10);
        let labels: HashSet<usize> = c.values().copied().collect();
        assert_eq!(labels.len(), 1, "a triangle is one community");
    }

    #[test]
    fn single_edge_merges_endpoints() {
        let e = edges(&[("a", "b")]);
        let c = communities(&e, 10);
        assert_eq!(c["a"], c["b"], "a single edge joins its endpoints");
    }

    #[test]
    fn disconnected_cliques_form_separate_communities() {
        // Two triangles with no bridge between them.
        let e = edges(&[
            ("a1", "a2"),
            ("a2", "a3"),
            ("a1", "a3"),
            ("b1", "b2"),
            ("b2", "b3"),
            ("b1", "b3"),
        ]);
        let c = communities(&e, 20);
        assert_eq!(c["a1"], c["a2"]);
        assert_eq!(c["a2"], c["a3"]);
        assert_eq!(c["b1"], c["b2"]);
        assert_eq!(c["b2"], c["b3"]);
        assert_ne!(c["a1"], c["b1"], "the two cliques stay separate");
    }

    #[test]
    fn empty_graph_returns_empty() {
        let c = communities::<String>(&[], 10);
        assert!(c.is_empty());
    }
}
