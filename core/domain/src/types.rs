//! Shared contract primitives.
//!
//! Identifiers are opaque strings, timestamps serialize as RFC3339 UTC through
//! `chrono`, and JSON-compatible payloads are reserved for genuinely flexible
//! scalar or metadata values. Core semantics should use typed fields instead of
//! hiding behavior in metadata.

use std::{collections::BTreeMap, fmt};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub type Timestamp = DateTime<Utc>;
pub type Metadata = BTreeMap<String, Value>;
pub type Scalar = Value;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    /// Creates an opaque identifier after rejecting empty values.
    ///
    /// The identifier is intentionally not parsed or interpreted here. Tenant,
    /// timestamp, authorization, and storage-location semantics must live in
    /// typed fields rather than inside the ID string.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainValidationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainValidationError::EmptyIdentifier);
        }
        Ok(Self(value))
    }

    /// Returns the raw identifier string for serialization and adapter lookup.
    ///
    /// Callers may compare this value for equality, but they should not derive
    /// scope, storage, or ordering behavior from its contents.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for Id {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Id {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainValidationError {
    #[error("identifier must not be empty")]
    EmptyIdentifier,
    #[error("confidence must be between 0 and 1")]
    InvalidConfidence,
}

pub type ActorId = Id;
pub type BeliefId = Id;
pub type ChunkId = Id;
pub type ConceptId = Id;
pub type ConceptSchemeId = Id;
pub type ConsolidationRunId = Id;
pub type ContradictionId = Id;
pub type DocumentId = Id;
pub type EntityId = Id;
pub type EvaluationId = Id;
pub type EventId = Id;
pub type HierarchyNodeId = Id;
pub type KnowledgeGraphId = Id;
pub type MemoryId = Id;
pub type OntologyAxiomId = Id;
pub type OntologyClassId = Id;
pub type OntologyId = Id;
pub type OntologyPropertyId = Id;
pub type RelationshipId = Id;
pub type SourceId = Id;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<EntityId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConceptRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<ConceptId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<OntologyId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Confidence(pub f32);

impl Confidence {
    /// Creates a normalized confidence value in the inclusive range `0..=1`.
    ///
    /// Confidence is a contract signal used by ranking, belief synthesis, and
    /// evaluation. Values outside the normalized range are rejected at the
    /// boundary instead of being clamped silently.
    pub fn new(value: f32) -> Result<Self, DomainValidationError> {
        if !(0.0..=1.0).contains(&value) {
            return Err(DomainValidationError::InvalidConfidence);
        }
        Ok(Self(value))
    }

    /// Returns the normalized confidence value for scoring and serialization.
    ///
    /// This accessor avoids exposing the tuple field as mutable state while
    /// keeping the type cheap to copy through ranking code.
    pub fn get(&self) -> f32 {
        self.0
    }
}
