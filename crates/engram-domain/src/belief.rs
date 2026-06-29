//! Derived belief and contradiction records.
//!
//! Beliefs are the layer's current stance over evidence, not source truth.
//! Contradictions are reviewable records that expose tension between evidence
//! without silently overwriting memory, chunks, entities, or prior beliefs.

use serde::{Deserialize, Serialize};

use crate::{
    Actor, BeliefId, ConceptRef, ContradictionId, DerivationRef, EmbeddingRef, EntityRef, Metadata,
    Policy, Provenance, Scope, Timestamp,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeliefSubject {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_ref: Option<EntityRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concept_ref: Option<ConceptRef>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeliefSourceTargetType {
    Memory,
    Assertion,
    Event,
    Chunk,
    Entity,
    Relationship,
    Concept,
    Belief,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeliefSource {
    pub target_type: BeliefSourceTargetType,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeliefStatus {
    Active,
    Stale,
    Superseded,
    Retracted,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Belief {
    pub id: BeliefId,
    pub scope: Scope,
    pub subject: BeliefSubject,
    pub content: String,
    pub status: BeliefStatus,
    pub confidence: f32,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sources: Vec<BeliefSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<BeliefId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthesizer: Option<DerivationRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub embedding_refs: Vec<EmbeddingRef>,
    pub policy: Policy,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionKind {
    Logical,
    Temporal,
    Tension,
    Duplicate,
    Policy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionStatus {
    Open,
    Resolved,
    Ignored,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionTargetType {
    Belief,
    Memory,
    Assertion,
    Chunk,
    Entity,
    Relationship,
    Concept,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContradictionTarget {
    pub target_type: ContradictionTargetType,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionResolutionKind {
    TargetWon,
    Compatible,
    Merged,
    Retracted,
    ManualIgnore,
    NeedsMoreEvidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContradictionResolution {
    pub kind: ContradictionResolutionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winning_target_id: Option<String>,
    pub actor: Actor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub resolved_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contradiction {
    pub id: ContradictionId,
    pub scope: Scope,
    pub kind: ContradictionKind,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub targets: Vec<ContradictionTarget>,
    pub severity: f32,
    pub status: ContradictionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_by: Option<DerivationRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<ContradictionResolution>,
    pub provenance: Provenance,
    pub detected_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
}
