//! Durable memory records and lifecycle events.
//!
//! `MemoryRecord` is the canonical agent-memory unit. It carries content,
//! scope, provenance, policy, status, links, and optional assertions while
//! remaining independent of how a store persists or indexes it.

use serde::{Deserialize, Serialize};

use crate::{EntityRef, EventId, MemoryId, Metadata, Policy, Provenance, Scalar, Scope, Timestamp};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Observation,
    Fact,
    Preference,
    Episode,
    Artifact,
    Relationship,
    Procedure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Archived,
    Redacted,
    Forgotten,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryContentFormat {
    Text,
    Markdown,
    Json,
    Code,
    Structured,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryContent {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub entities: Vec<EntityRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<MemoryContentFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured: Option<Scalar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryAssertion {
    pub subject: EntityRef,
    pub predicate: String,
    pub object: Scalar,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkTargetType {
    Memory,
    Event,
    Belief,
    Contradiction,
    Chunk,
    Document,
    Entity,
    Concept,
    HierarchyNode,
    Source,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryLink {
    pub rel: String,
    pub target_type: LinkTargetType,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecord {
    pub id: MemoryId,
    pub kind: MemoryKind,
    pub content: MemoryContent,
    pub scope: Scope,
    pub provenance: Provenance,
    pub policy: Policy,
    pub status: MemoryStatus,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub links: Vec<MemoryLink>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub assertions: Vec<MemoryAssertion>,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEventKind {
    Observed,
    Written,
    Updated,
    Retrieved,
    Consolidated,
    Redacted,
    Forgotten,
    Expired,
    PolicyChanged,
    Linked,
    Unlinked,
    BeliefSynthesized,
    BeliefRetracted,
    ContradictionDetected,
    HierarchyBuilt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEvent {
    pub id: EventId,
    pub kind: MemoryEventKind,
    pub scope: Scope,
    pub actor: crate::Actor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<MemoryId>,
    pub payload: Scalar,
    pub provenance: Provenance,
    pub occurred_at: Timestamp,
    pub recorded_at: Timestamp,
}
