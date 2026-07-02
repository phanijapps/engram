//! Hierarchy contracts for aggregation, navigation, and context compression.
//!
//! A hierarchy node may represent a base retrievable object or an aggregate.
//! These types preserve enough provenance to explain how a hierarchy was built
//! without baking in a clustering algorithm, vector store, or model provider.

use serde::{Deserialize, Serialize};

use crate::{
    ConsolidationError, EmbeddingRef, EvidenceRef, HierarchyNodeId, Metadata, Policy, Provenance,
    RetrievalTargetType, Scope, Timestamp,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HierarchyNodeKind {
    Base,
    Aggregate,
    Schema,
    Topic,
    Cluster,
    Domain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HierarchyNodeStatus {
    Active,
    Stale,
    Archived,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HierarchyMemberType {
    HierarchyNode,
    Memory,
    Event,
    Chunk,
    Entity,
    Relationship,
    Concept,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyMembership {
    pub id: String,
    pub parent_id: HierarchyNodeId,
    pub member_type: HierarchyMemberType,
    pub member_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    pub provenance: Provenance,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyNode {
    pub id: HierarchyNodeId,
    pub scope: Scope,
    pub kind: HierarchyNodeKind,
    pub layer: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<HierarchyNodeId>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub members: Vec<HierarchyMembership>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_target_type: Option<RetrievalTargetType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_target_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub embedding_refs: Vec<EmbeddingRef>,
    pub status: HierarchyNodeStatus,
    pub policy: Policy,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyRelation {
    pub id: String,
    pub scope: Scope,
    pub source_id: HierarchyNodeId,
    pub target_id: HierarchyNodeId,
    pub predicate: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_inter_cluster: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub evidence: Vec<EvidenceRef>,
    pub provenance: Provenance,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyBuildConfig {
    pub id: String,
    pub algorithm: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_cluster_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_layers: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity_metric: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inter_cluster_threshold: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_budget: Option<u32>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HierarchyBuildStatus {
    Running,
    Completed,
    CompletedWithErrors,
    Failed,
    Cancelled,
}

/// Auditable record for one hierarchy construction run.
///
/// This draft extension model documents builder inputs and outputs without
/// prescribing a storage table or build algorithm. Adapters may persist it
/// directly later; today nodes and relations carry enough provenance to link
/// back to a build record id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyBuildRecord {
    pub id: String,
    pub scope: Scope,
    pub config: HierarchyBuildConfig,
    pub status: HierarchyBuildStatus,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub input_refs: Vec<EvidenceRef>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub output_node_ids: Vec<HierarchyNodeId>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub output_relation_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<Metadata>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub errors: Vec<ConsolidationError>,
    pub provenance: Provenance,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyPath {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub seed_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lca_id: Option<HierarchyNodeId>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub nodes: Vec<HierarchyNode>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub relations: Vec<HierarchyRelation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_layer: Option<u32>,
}
