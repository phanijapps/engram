//! Pure associative ranking over a directed edge list.
//!
//! Builds a bidirected edge set from the supplied relationships and ranks nodes
//! by Personalized PageRank seeded at the given seed keys. Associative recall is
//! direction-agnostic for this slice, so each relationship contributes both
//! `(subject, object)` and `(object, subject)` — symmetric predicates like
//! `related_to` then carry mass both ways, and an entity reached only as an
//! `object` is still reachable. Predicate-directionality policy is a deferred
//! Ask-first item.
//!
//! The helper is pure over `String` node keys so it is unit-testable without
//! constructing domain types; the `RetrievalIndex` glue (see `index.rs`) maps
//! `KnowledgeRelationship` to these edges at the call boundary using the same
//! node-key function as seed resolution, so seed keys and edge endpoints share
//! one key space.

use engram_graph_analytics::personalized_pagerank;

/// Personalized PageRank configuration.
#[derive(Debug, Clone, Copy)]
pub struct PprConfig {
    pub damping: f64,
    pub iterations: usize,
    pub tol: f64,
}

impl Default for PprConfig {
    fn default() -> Self {
        Self {
            damping: 0.85,
            iterations: 200,
            tol: 1e-9,
        }
    }
}

/// Ranks nodes by Personalized PageRank seeded at `seeds`, over a bidirected
/// view of `edges`.
///
/// `edges` are `(source, target)` directed pairs; each is also walked in reverse
/// so the walk is effectively undirected. Returns `(node_key, score)` pairs
/// sorted by score descending, then node key ascending for determinism. Returns
/// an empty vec when `seeds` is empty. Seeds absent from every edge still appear,
/// because they are part of the PPR node set.
pub fn rank_associative(
    edges: &[(String, String)],
    seeds: &[String],
    config: PprConfig,
) -> Vec<(String, f64)> {
    if seeds.is_empty() {
        return Vec::new();
    }
    let mut bidirected: Vec<(String, String)> = Vec::with_capacity(edges.len().saturating_mul(2));
    for (s, o) in edges {
        bidirected.push((s.clone(), o.clone()));
        // Skip the reverse for self-loops so a `subject == object` relationship
        // does not double the node's out-degree.
        if s != o {
            bidirected.push((o.clone(), s.clone()));
        }
    }
    let scores = personalized_pagerank(
        &bidirected,
        seeds,
        config.damping,
        config.iterations,
        config.tol,
    );
    let mut ranked: Vec<(String, f64)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(s: &str, o: &str) -> (String, String) {
        (s.to_string(), o.to_string())
    }

    fn seeds(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn ordered_ids(ranked: &[(String, f64)]) -> Vec<String> {
        ranked.iter().map(|(id, _)| id.clone()).collect()
    }

    #[test]
    fn proximity_seeds_and_neighbors_outrank_distant_nodes() {
        // Undirected path a - b - c - d (bidirected walk), seeded at a. The seed
        // and its 1-hop neighborhood (a, b) outrank the distant nodes (c, d), and
        // the farthest node d ranks last. On an undirected graph a higher-degree
        // node like b can outrank the seed itself — that is correct PPR behavior,
        // so this test asserts proximity ordering, not strict seed-first ordering.
        let edges = vec![edge("a", "b"), edge("b", "c"), edge("c", "d")];
        let ranked = rank_associative(&edges, &seeds(&["a"]), PprConfig::default());
        let score_of = |id: &str| -> f64 {
            ranked
                .iter()
                .find(|(k, _)| k == id)
                .map(|(_, s)| *s)
                .unwrap_or(-1.0)
        };
        // 1-hop-or-closer (a, b) outrank 2+-hop (c, d).
        for close in ["a", "b"] {
            for far in ["c", "d"] {
                assert!(
                    score_of(close) > score_of(far),
                    "{close} ({}) should outrank {far} ({})",
                    score_of(close),
                    score_of(far)
                );
            }
        }
        // The farthest node ranks last.
        assert_eq!(ranked.last().map(|(id, _)| id.as_str()), Some("d"));
    }

    #[test]
    fn bidirected_walk_reaches_reverse_neighbor() {
        // Single directed edge a -> b, seeded at b. The bidirected walk makes a
        // reachable, ranking just below the seed.
        let edges = vec![edge("a", "b")];
        let ranked = rank_associative(&edges, &seeds(&["b"]), PprConfig::default());
        assert_eq!(ordered_ids(&ranked), vec!["b", "a"]);
        assert!(ranked[0].1 > ranked[1].1, "seed b must outrank a");
    }

    #[test]
    fn isolated_seed_appears_with_mass() {
        // Seed z appears in no edge but is still part of the node set.
        let edges = vec![edge("a", "b")];
        let ranked = rank_associative(&edges, &seeds(&["z"]), PprConfig::default());
        assert!(
            ranked.iter().any(|(id, _)| id == "z"),
            "isolated seed z must appear"
        );
    }

    #[test]
    fn no_seeds_returns_empty() {
        let edges = vec![edge("a", "b")];
        let ranked = rank_associative(&edges, &seeds(&[]), PprConfig::default());
        assert!(ranked.is_empty());
    }

    #[test]
    fn only_seeds_and_edge_endpoints_appear() {
        // "ghost" is neither a seed nor an edge endpoint, so it must not appear.
        let edges = vec![edge("a", "b"), edge("b", "c")];
        let ranked = rank_associative(&edges, &seeds(&["a"]), PprConfig::default());
        let ids = ordered_ids(&ranked);
        assert!(
            !ids.iter().any(|id| id == "ghost"),
            "the walk must not invent nodes"
        );
        assert!(ids.contains(&"a".to_string()));
        assert!(ids.contains(&"b".to_string()));
        assert!(ids.contains(&"c".to_string()));
    }

    #[test]
    fn deterministic_across_runs() {
        let edges = vec![
            edge("a", "b"),
            edge("b", "c"),
            edge("c", "a"),
            edge("a", "c"),
        ];
        let first = rank_associative(&edges, &seeds(&["a"]), PprConfig::default());
        let second = rank_associative(&edges, &seeds(&["a"]), PprConfig::default());
        assert_eq!(first, second);
    }
}
