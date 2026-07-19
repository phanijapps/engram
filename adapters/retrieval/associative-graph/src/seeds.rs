//! Deterministic lexical seed resolution.
//!
//! Maps a retrieval query to the entity node keys the Personalized PageRank walk
//! personalizes on. Resolution is lexical and model-free: query tokens
//! (lowercased, stripped to alphanumeric) are matched against entity names —
//! exact match first, then token containment — so the seed set is fully
//! deterministic for a given (query, entities) pair. No LLM, no embeddings.
//!
//! Operates on `(node_key, name)` pairs so it is unit-testable without
//! constructing domain types; the `RetrievalIndex` glue (see `index.rs`) maps
//! `KnowledgeEntity` to these pairs at the call boundary.

use std::collections::HashSet;

/// Resolves seed node keys from a query by matching query tokens to entity
/// names.
///
/// `entities` is a list of `(node_key, name)` pairs. Returns the node keys of
/// entities whose name matches a query token (exact match preferred, otherwise
/// token containment), de-duplicated and in iteration order. Returns an empty
/// vec when the query has no alphanumeric tokens.
///
/// Note: very short tokens (e.g. a single letter) can over-seed via containment
/// (matching many entity names). A token-length floor is a tuning concern for
/// the wiring slice, not this adapter unit.
pub fn resolve_seeds(query: &str, entities: &[(String, String)]) -> Vec<String> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut seeds: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (key, name) in entities {
        let lower = name.to_ascii_lowercase();
        let matches =
            tokens.iter().any(|t| t == &lower) || tokens.iter().any(|t| lower.contains(t.as_str()));
        if matches && seen.insert(key.clone()) {
            seeds.push(key.clone());
        }
    }
    seeds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(id: &str, name: &str) -> (String, String) {
        (id.to_string(), name.to_string())
    }

    #[test]
    fn exact_name_match_resolves_seed() {
        let entities = vec![pair("a", "Alice"), pair("b", "Bob")];
        let seeds = resolve_seeds("Alice", &entities);
        assert_eq!(seeds, vec!["a".to_string()]);
    }

    #[test]
    fn case_insensitive_match_resolves_seed() {
        let entities = vec![pair("a", "Alice Engine")];
        let seeds = resolve_seeds("alice", &entities);
        assert_eq!(seeds, vec!["a".to_string()]);
    }

    #[test]
    fn token_containment_match_resolves_seed() {
        // "eng" is a substring of "engram".
        let entities = vec![pair("e", "engram")];
        let seeds = resolve_seeds("eng", &entities);
        assert_eq!(seeds, vec!["e".to_string()]);
    }

    #[test]
    fn no_match_returns_empty() {
        let entities = vec![pair("a", "Alice")];
        let seeds = resolve_seeds("zzz", &entities);
        assert!(seeds.is_empty());
    }

    #[test]
    fn empty_query_returns_empty() {
        let entities = vec![pair("a", "Alice")];
        let seeds = resolve_seeds("   !! ", &entities);
        assert!(seeds.is_empty());
    }

    #[test]
    fn multiple_matches_dedup_and_preserve_order() {
        let entities = vec![
            pair("a", "Alice"),
            pair("b", "Alice Clone"),
            pair("a", "Alice"), // duplicate key — deduped
        ];
        let seeds = resolve_seeds("Alice", &entities);
        assert_eq!(seeds, vec!["a".to_string(), "b".to_string()]);
    }
}
