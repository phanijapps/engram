//! Betweenness centrality over a directed edge list (Brandes' algorithm).

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// Computes betweenness centrality for the nodes of a directed graph.
///
/// `edges` are `(source, target)` id pairs. Betweenness counts how many
/// shortest paths pass through each node; endpoints of the only path through a
/// node are not credited, so pure sources/sinks score 0. Uses Brandes'
/// algorithm. Returns a map of node id -> betweenness.
pub fn betweenness<N>(edges: &[(N, N)]) -> HashMap<N, f64>
where
    N: Eq + Hash + Clone,
{
    // Assign each node a dense integer index so the BFS/accumulation works over
    // plain `Vec`s rather than borrowed `HashMap` lookups.
    let mut index_of: HashMap<N, usize> = HashMap::new();
    let mut nodes: Vec<N> = Vec::new();
    let mut adj: Vec<Vec<usize>> = Vec::new();
    for (source, target) in edges {
        let si = ensure(source, &mut index_of, &mut nodes, &mut adj);
        let ti = ensure(target, &mut index_of, &mut nodes, &mut adj);
        adj[si].push(ti);
    }

    let n = nodes.len();
    let mut betw = vec![0.0_f64; n];

    for s in 0..n {
        // Single-source shortest-path BFS.
        let mut stack: Vec<usize> = Vec::new();
        let mut preds: Vec<Vec<usize>> = (0..n).map(|_| Vec::new()).collect();
        let mut sigma = vec![0.0_f64; n];
        let mut dist = vec![-1_i64; n];
        let mut queue: VecDeque<usize> = VecDeque::new();
        sigma[s] = 1.0;
        dist[s] = 0;
        queue.push_back(s);

        while let Some(v) = queue.pop_front() {
            stack.push(v);
            let dv = dist[v];
            let sv = sigma[v];
            for &w in &adj[v] {
                if dist[w] < 0 {
                    dist[w] = dv + 1;
                    queue.push_back(w);
                }
                if dist[w] == dv + 1 {
                    sigma[w] += sv;
                    preds[w].push(v);
                }
            }
        }

        // Dependency accumulation (reverse order).
        let mut delta = vec![0.0_f64; n];
        while let Some(w) = stack.pop() {
            let sw = sigma[w];
            let dw = delta[w];
            for &v in &preds[w] {
                let contrib = (sigma[v] / sw) * (1.0 + dw);
                delta[v] += contrib;
            }
            if w != s {
                betw[w] += delta[w];
            }
        }
    }

    nodes.into_iter().zip(betw).collect()
}

fn ensure<N>(
    node: &N,
    index_of: &mut HashMap<N, usize>,
    nodes: &mut Vec<N>,
    adj: &mut Vec<Vec<usize>>,
) -> usize
where
    N: Eq + Hash + Clone,
{
    if let Some(&i) = index_of.get(node) {
        return i;
    }
    let i = nodes.len();
    nodes.push(node.clone());
    adj.push(Vec::new());
    index_of.insert(node.clone(), i);
    i
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
    fn bridge_node_carries_all_through_traffic() {
        // a -> b -> c: the only a->c path goes through b.
        let e = edges(&[("a", "b"), ("b", "c")]);
        let bc = betweenness(&e);
        assert!(bc["b"] > bc["a"], "bridge b must outrank source a");
        assert!(bc["b"] > bc["c"], "bridge b must outrank sink c");
        assert_eq!(bc["a"], 0.0);
        assert_eq!(bc["c"], 0.0);
    }

    #[test]
    fn parallel_paths_split_betweenness() {
        // a->{b,c}->d: two equal shortest a->d paths; b and c split the credit.
        let e = edges(&[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")]);
        let bc = betweenness(&e);
        assert!((bc["b"] - 0.5).abs() < 1e-9, "b gets half: {}", bc["b"]);
        assert!((bc["c"] - 0.5).abs() < 1e-9, "c gets half: {}", bc["c"]);
        assert_eq!(bc["a"], 0.0);
        assert_eq!(bc["d"], 0.0);
    }

    #[test]
    fn empty_graph_returns_empty() {
        let bc = betweenness::<String>(&[]);
        assert!(bc.is_empty());
    }

    #[test]
    fn deterministic_across_runs() {
        let e = edges(&[("a", "b"), ("b", "c"), ("a", "c"), ("c", "a")]);
        let first = betweenness(&e);
        let second = betweenness(&e);
        assert_eq!(first, second);
    }
}
