//! SQLite-backed [`CommunityQuery`] — the Louvain + meta-edge + member-index
//! aggregate, computed from `SqlKnowledgeStore`'s relationships via
//! `engram_graph-analytics`. Engine-specific (names `SqlKnowledgeStore`, gated
//! behind the `sqlite` feature). Stateless: the caller (viz) caches the result
//! per store-version; the port recomputes on each call.
//!
//! This is the Rust home of the aggregate logic that previously lived as
//! `node:sqlite` relationship streaming + `call_communities` in the viz BFF.

use std::collections::HashMap;

use async_trait::async_trait;
use engram_domain::{CommunityMetaEdge, CommunityMetaNode, CommunityOverview, EntityRef, Scope};
use engram_graph_analytics::communities;
use engram_runtime::CoreResult;
use engram_store_sqlite::SqlKnowledgeStore;

use crate::community_query::CommunityQuery;

const MEMBER_CAP: usize = 1000;

/// name ?? id — the key `communities()` groups by (entity_key semantics).
fn entity_key(e: &EntityRef) -> Option<String> {
    e.name
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| e.id.as_ref().map(|id| id.to_string()))
}

#[async_trait]
impl CommunityQuery for SqlKnowledgeStore {
    async fn overview(&self, scope: &Scope, limit: usize) -> CoreResult<CommunityOverview> {
        let rels = SqlKnowledgeStore::list_relationships(self, scope).await?;

        // Louvain edge list (name ?? id).
        let edges: Vec<(String, String)> = rels
            .iter()
            .filter_map(|r| {
                let s = entity_key(&r.subject)?;
                let o = entity_key(&r.object)?;
                Some((s, o))
            })
            .collect();
        let name_to_label = communities(&edges, 2);
        let total_communities = name_to_label.len();

        // Rank communities by membership.
        let mut counts: HashMap<usize, usize> = HashMap::new();
        for label in name_to_label.values() {
            *counts.entry(*label).or_default() += 1;
        }
        let mut ranked: Vec<(usize, usize)> = counts.into_iter().collect();
        ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        let top_labels: std::collections::HashSet<usize> =
            ranked.iter().take(limit).map(|(l, _)| *l).collect();

        let communities_nodes: Vec<CommunityMetaNode> = ranked
            .iter()
            .take(limit)
            .map(|(label, member_count)| CommunityMetaNode {
                label: *label as u32,
                member_count: *member_count,
            })
            .collect();

        // Meta-edges: tally inter-community label pairs from the edges.
        let mut edge_weights: HashMap<(usize, usize), usize> = HashMap::new();
        for (s, o) in &edges {
            let ls = match name_to_label.get(s) {
                Some(l) => *l,
                None => continue,
            };
            let lo = match name_to_label.get(o) {
                Some(l) => *l,
                None => continue,
            };
            if ls == lo || !top_labels.contains(&ls) || !top_labels.contains(&lo) {
                continue;
            }
            let key = if ls < lo { (ls, lo) } else { (lo, ls) };
            *edge_weights.entry(key).or_default() += 1;
        }
        let mut meta_edges: Vec<CommunityMetaEdge> = edge_weights
            .into_iter()
            .map(|((a, b), weight)| CommunityMetaEdge {
                source_label: a as u32,
                target_label: b as u32,
                weight,
            })
            .collect();
        meta_edges.sort_unstable_by(|a, b| b.weight.cmp(&a.weight));
        let max_edges = communities_nodes.len().saturating_mul(4);
        meta_edges.truncate(max_edges);

        Ok(CommunityOverview {
            communities: communities_nodes,
            edges: meta_edges,
            total_communities,
        })
    }

    async fn member_index(&self, scope: &Scope) -> CoreResult<HashMap<u32, Vec<String>>> {
        let rels = SqlKnowledgeStore::list_relationships(self, scope).await?;
        let edges: Vec<(String, String)> = rels
            .iter()
            .filter_map(|r| {
                let s = entity_key(&r.subject)?;
                let o = entity_key(&r.object)?;
                Some((s, o))
            })
            .collect();
        let name_to_label = communities(&edges, 2);

        let mut label_to_ids: HashMap<usize, Vec<String>> = HashMap::new();
        for r in &rels {
            for (e, _is_subject) in [(&r.subject, true), (&r.object, false)] {
                let key = match entity_key(e) {
                    Some(k) => k,
                    None => continue,
                };
                let label = match name_to_label.get(&key) {
                    Some(l) => *l,
                    None => continue,
                };
                let id = match &e.id {
                    Some(id) => id.to_string(),
                    None => continue,
                };
                let bucket = label_to_ids.entry(label).or_default();
                if bucket.len() < MEMBER_CAP {
                    bucket.push(id);
                }
            }
        }
        Ok(label_to_ids.into_iter().map(|(l, ids)| (l as u32, ids)).collect())
    }

    async fn community_of(&self, scope: &Scope, entity_id: &str) -> CoreResult<Option<u32>> {
        let index = Box::pin(SqlKnowledgeStore::member_index(self, scope)).await?;
        for (label, ids) in &index {
            if ids.iter().any(|id| id == entity_id) {
                return Ok(Some(*label));
            }
        }
        Ok(None)
    }
}
