//! Community-overview aggregate result types.
//!
//! Engine-neutral shapes returned by a `CommunityOverview` port (the Louvain
//! community structure + inter-community meta-edges derived from the knowledge
//! graph). These carry only the data; the consumer (viz) maps `label` to a
//! display id/name and applies a deterministic layout (a view concern), so no
//! positions or display conventions live here.

use serde::{Deserialize, Serialize};

/// A Louvain community aggregate node: the numeric label + how many entities
/// cluster under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityMetaNode {
    pub label: u32,
    pub member_count: usize,
}

/// An inter-community meta-edge: how many relationships span two communities,
/// undirected (`source_label`/`target_label` ordered only for a stable key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityMetaEdge {
    pub source_label: u32,
    pub target_label: u32,
    pub weight: usize,
}

/// The community-overview payload: the top-N communities (by membership) + the
/// bounded inter-community meta-edges + the total community count (pre-truncation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityOverview {
    pub communities: Vec<CommunityMetaNode>,
    pub edges: Vec<CommunityMetaEdge>,
    pub total_communities: usize,
}

/// Scope-filtered record counts for the stats endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeCounts {
    pub entities: usize,
    pub relationships: usize,
    pub memories: usize,
    pub beliefs: usize,
    pub hierarchy_nodes: usize,
    pub hierarchy_relations: usize,
}
