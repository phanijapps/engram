//! Durable memory records and lifecycle events.
//!
//! `MemoryRecord` is the canonical agent-memory unit. It carries content,
//! scope, provenance, policy, status, links, and optional assertions while
//! remaining independent of how a store persists or indexes it. Memory roles
//! are derived helpers over accepted v1 fields; they do not add a `role` wire
//! field to `MemoryRecord`.

use serde::{Deserialize, Serialize};

use crate::{
    EntityRef, EventId, MemoryId, Metadata, Policy, Provenance, Retention, Scalar, Scope, Timestamp,
};

/// Portable category for a stored memory record.
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

impl MemoryKind {
    /// Returns the default architecture role implied by this v1 memory kind.
    ///
    /// Working memory is intentionally absent from this mapping because live
    /// working context is host-owned. Use [`MemoryRole::for_record`] when scope
    /// and retention policy are available; it can classify session-bounded
    /// observations and episodes as working-memory evictions/traces.
    pub fn default_role(&self) -> MemoryRole {
        match self {
            Self::Observation | Self::Episode => MemoryRole::Episodic,
            Self::Fact | Self::Preference | Self::Artifact | Self::Relationship => {
                MemoryRole::Semantic
            }
            Self::Procedure => MemoryRole::Procedural,
        }
    }
}

/// Draft architecture role derived from accepted v1 memory fields.
///
/// This enum lets Rust code and tests align with the research architecture's
/// working/episodic/semantic/procedural taxonomy without changing the accepted
/// v1 wire contract. Adapters should not persist this as hidden metadata; if a
/// future wire field is needed, it must go through contract review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRole {
    Working,
    Episodic,
    Semantic,
    Procedural,
}

impl MemoryRole {
    /// Classifies a memory record using kind, retention policy, and session
    /// scope.
    ///
    /// Session-bounded ephemeral/session observations and episodes represent
    /// persisted working-memory traces or evictions. All other records fall
    /// back to the role implied by their `MemoryKind`.
    pub fn for_record(record: &MemoryRecord) -> Self {
        if matches!(record.kind, MemoryKind::Observation | MemoryKind::Episode)
            && matches!(
                record.policy.retention,
                Retention::Ephemeral | Retention::Session
            )
            && record.scope.session.is_some()
        {
            Self::Working
        } else {
            record.kind.default_role()
        }
    }
}

/// Lifecycle state for a memory record.
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

impl MemoryRecord {
    /// Returns the architecture role derived from accepted v1 fields.
    pub fn role(&self) -> MemoryRole {
        MemoryRole::for_record(self)
    }
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
