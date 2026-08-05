//! SQLite-backed [`CommunityQuery`] — Louvain + meta-edges + member index via
//! `engram-graph-analytics`, using lightweight `relationship_endpoints`.

use std::collections::HashMap;

use async_trait::async_trait;
use engram_domain::{CommunityMetaEdge, CommunityMetaNode, CommunityOverview, Scope, ScopeCounts};
use engram_graph_analytics::communities;
use engram_runtime::CoreResult;
use engram_store_sqlite::SqlKnowledgeStore;

use crate::community_query::CommunityQuery;

const MEMBER_CAP: usize = 1000;

fn key_of(name: Option<&str>, id: Option<&str>) -> Option<String> {
    name.filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| id.map(String::from))
}

fn endpoint_key(name: &Option<String>, id: &Option<String>) -> Option<String> {
    key_of(name.as_deref(), id.as_deref())
}

#[async_trait]
impl CommunityQuery for SqlKnowledgeStore {
    async fn overview(&self, scope: &Scope, limit: usize) -> CoreResult<CommunityOverview> {
        let rels = SqlKnowledgeStore::relationship_endpoints(self, scope).await?;
        let edges: Vec<(String, String)> = rels
            .iter()
            .filter_map(|(sn, si, on, oi)| {
                Some((endpoint_key(sn, si)?, endpoint_key(on, oi)?))
            })
            .collect();
        let name_to_label = communities(&edges, 2);
        let total_communities = name_to_label.len();

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
            .map(|(label, mc)| CommunityMetaNode {
                label: *label as u32,
                member_count: *mc,
            })
            .collect();

        let mut ew: HashMap<(usize, usize), usize> = HashMap::new();
        for (s, o) in &edges {
            let (Some(ls), Some(lo)) = (name_to_label.get(s), name_to_label.get(o)) else {
                continue;
            };
            let (ls, lo) = (*ls, *lo);
            if ls == lo || !top_labels.contains(&ls) || !top_labels.contains(&lo) {
                continue;
            }
            *ew.entry(if ls < lo { (ls, lo) } else { (lo, ls) })
                .or_default() += 1;
        }
        let mut meta_edges: Vec<CommunityMetaEdge> = ew
            .into_iter()
            .map(|((a, b), w)| CommunityMetaEdge {
                source_label: a as u32,
                target_label: b as u32,
                weight: w,
            })
            .collect();
        meta_edges.sort_unstable_by(|a, b| b.weight.cmp(&a.weight));
        meta_edges.truncate(communities_nodes.len().saturating_mul(4));

        Ok(CommunityOverview {
            communities: communities_nodes,
            edges: meta_edges,
            total_communities,
        })
    }

    async fn member_index(&self, scope: &Scope) -> CoreResult<HashMap<u32, Vec<String>>> {
        let rels = SqlKnowledgeStore::relationship_endpoints(self, scope).await?;
        let edges: Vec<(String, String)> = rels
            .iter()
            .filter_map(|(sn, si, on, oi)| {
                Some((endpoint_key(sn, si)?, endpoint_key(on, oi)?))
            })
            .collect();
        let name_to_label = communities(&edges, 2);

        let mut label_to_ids: HashMap<usize, Vec<String>> = HashMap::new();
        for (sn, si, on, oi) in &rels {
            // subject endpoint
            if let Some(key) = endpoint_key(sn, si) {
                if let Some(&label) = name_to_label.get(&key) {
                    if let Some(eid) = si {
                        let b = label_to_ids.entry(label).or_default();
                        if b.len() < MEMBER_CAP {
                            b.push(eid.clone());
                        }
                    }
                }
            }
            // object endpoint
            if let Some(key) = endpoint_key(on, oi) {
                if let Some(&label) = name_to_label.get(&key) {
                    if let Some(eid) = oi {
                        let b = label_to_ids.entry(label).or_default();
                        if b.len() < MEMBER_CAP {
                            b.push(eid.clone());
                        }
                    }
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

    async fn scope_counts(&self, scope: &Scope) -> CoreResult<ScopeCounts> {
        SqlKnowledgeStore::scope_counts(self, scope).await
    }
}
