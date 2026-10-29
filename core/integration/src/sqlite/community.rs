//! SQLite-backed [`CommunityQuery`] — Louvain + meta-edges + member index via
//! `engram-graph-analytics`, using lightweight `relationship_endpoints`.

use std::collections::HashMap;

use async_trait::async_trait;
use engram_domain::{CommunityMetaEdge, CommunityMetaNode, CommunityOverview, Scope, ScopeCounts};
use engram_graph_analytics::communities;
use engram_retrieval::BoundCodeView;
use engram_runtime::CoreResult;
use engram_store_sqlite::SqlKnowledgeStore;

use crate::community_query::CommunityQuery;

const MEMBER_CAP: usize = 1000;

/// Community node key. RFC-0020 rev: prefer the entity **id** (stable, unique
/// per `graph_id + name`) so two functions that share a logical name — two
/// `render`, two `index.ts` — stay distinct graph nodes instead of collapsing
/// into one. The logical name is still the display label (the cc display maps
/// ids→names); only the partitioning key changed. Falls back to the name only
/// when an endpoint carries no id (a name-only relationship ref).
fn key_of(name: Option<&str>, id: Option<&str>) -> Option<String> {
    id.filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| name.filter(|s| !s.is_empty()).map(String::from))
}

fn endpoint_key(name: &Option<String>, id: &Option<String>) -> Option<String> {
    key_of(name.as_deref(), id.as_deref())
}

/// Eligibility-constrained community overview (task_014): edges touching a
/// stale/retired/legacy-unbound code endpoint are removed BEFORE Louvain so a
/// retired generation neither shapes community structure nor leaks into a
/// mixed summary. Non-code endpoints always stay; an ordinary doc-only
/// community survives a code retirement intact. A free engine-specific
/// function (the orphan rules forbid an inherent `impl SqlKnowledgeStore`
/// here); the neutral `CommunityQuery` port stays unchanged.
pub async fn overview_eligible(
    store: &SqlKnowledgeStore,
    scope: &Scope,
    limit: usize,
    view: &BoundCodeView,
) -> CoreResult<CommunityOverview> {
    use engram_knowledge::KnowledgeRepository as _;
    let entities = store.list_entities(scope).await?;
    let mut removed: std::collections::HashSet<String> = Default::default();
    for entity in &entities {
        if !view.allows(entity.id.as_str(), entity_unstamped_code(entity)) {
            removed.insert(entity.id.to_string());
        }
    }
    let rels = SqlKnowledgeStore::relationship_endpoints(store, scope).await?;
    let edges: Vec<(String, String)> = rels
        .iter()
        .filter_map(|(sn, si, on, oi)| Some((endpoint_key(sn, si)?, endpoint_key(on, oi)?)))
        .filter(|(subject, object)| !removed.contains(subject) && !removed.contains(object))
        .collect();
    overview_from_edges(&edges, limit)
}

/// True for code-symbol / code-artifact entities without a generation stamp
/// (legacy unbound code — never eligible under a binding view).
fn entity_unstamped_code(entity: &engram_domain::KnowledgeEntity) -> bool {
    let stamped = entity
        .metadata
        .as_ref()
        .and_then(|meta| meta.get(engram_retrieval::bound_filter::CODE_GENERATION_KEY))
        .is_some();
    if stamped {
        return false;
    }
    engram_retrieval::is_code_artifact(entity)
        || matches!(
            entity.kind,
            engram_domain::EntityKind::Class
                | engram_domain::EntityKind::Function
                | engram_domain::EntityKind::Method
                | engram_domain::EntityKind::Struct
                | engram_domain::EntityKind::Interface
                | engram_domain::EntityKind::Trait
                | engram_domain::EntityKind::TypeAlias
                | engram_domain::EntityKind::Enum
        )
}

/// Shared overview computation (Louvain over the edge set + meta-edges),
/// used by the unconstrained and eligibility-constrained paths alike.
fn overview_from_edges(edges: &[(String, String)], limit: usize) -> CoreResult<CommunityOverview> {
    let name_to_label = communities(edges, 2);
    let total_communities = name_to_label.len();

    let mut counts: HashMap<usize, usize> = HashMap::new();
    for label in name_to_label.values() {
        *counts.entry(*label).or_default() += 1;
    }
    let mut ranked: Vec<(usize, usize)> = counts.into_iter().collect();
    ranked.sort_unstable_by_key(|a| std::cmp::Reverse(a.1));
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
    for (s, o) in edges {
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
    meta_edges.sort_unstable_by_key(|e| std::cmp::Reverse(e.weight));
    meta_edges.truncate(communities_nodes.len().saturating_mul(4));

    Ok(CommunityOverview {
        communities: communities_nodes,
        edges: meta_edges,
        total_communities,
    })
}

#[async_trait]
impl CommunityQuery for SqlKnowledgeStore {
    async fn overview(&self, scope: &Scope, limit: usize) -> CoreResult<CommunityOverview> {
        let rels = SqlKnowledgeStore::relationship_endpoints(self, scope).await?;
        let edges: Vec<(String, String)> = rels
            .iter()
            .filter_map(|(sn, si, on, oi)| Some((endpoint_key(sn, si)?, endpoint_key(on, oi)?)))
            .collect();
        overview_from_edges(&edges, limit)
    }

    async fn member_index(&self, scope: &Scope) -> CoreResult<HashMap<u32, Vec<String>>> {
        let rels = SqlKnowledgeStore::relationship_endpoints(self, scope).await?;
        let edges: Vec<(String, String)> = rels
            .iter()
            .filter_map(|(sn, si, on, oi)| Some((endpoint_key(sn, si)?, endpoint_key(on, oi)?)))
            .collect();
        let name_to_label = communities(&edges, 2);

        let mut label_to_ids: HashMap<usize, Vec<String>> = HashMap::new();
        // Container IDs (graph-*, repo-*) are structural nodes, not code
        // symbols — including them in the member index means communities
        // hydrate to nothing (no matching entity rows). Only symbol entities
        // (entity-*) are community members.
        let is_symbol = |id: &str| !id.starts_with("graph-") && !id.starts_with("repo-");
        for (sn, si, on, oi) in &rels {
            // subject endpoint — push entity id if not already in the community
            if let Some(key) = endpoint_key(sn, si) {
                if let Some(&label) = name_to_label.get(&key) {
                    if let Some(eid) = si {
                        if is_symbol(eid) {
                            let b = label_to_ids.entry(label).or_default();
                            if b.len() < MEMBER_CAP && !b.contains(eid) {
                                b.push(eid.clone());
                            }
                        }
                    }
                }
            }
            // object endpoint
            if let Some(key) = endpoint_key(on, oi) {
                if let Some(&label) = name_to_label.get(&key) {
                    if let Some(eid) = oi {
                        if is_symbol(eid) {
                            let b = label_to_ids.entry(label).or_default();
                            if b.len() < MEMBER_CAP && !b.contains(eid) {
                                b.push(eid.clone());
                            }
                        }
                    }
                }
            }
        }
        Ok(label_to_ids
            .into_iter()
            .map(|(l, ids)| (l as u32, ids))
            .collect())
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
