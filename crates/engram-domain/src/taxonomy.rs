//! Taxonomy contracts for controlled concepts and their evolution.
//!
//! These models are SKOS-aligned where useful, but remain local to engram's
//! contract. Direct relations are stored explicitly; ancestor, descendant, and
//! cross-scheme views should be computed by taxonomy or hierarchy services.

use serde::{Deserialize, Serialize};

use crate::{Actor, ConceptId, ConceptSchemeId, Policy, Provenance, Scalar, Scope, Timestamp};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConceptScheme {
    pub id: ConceptSchemeId,
    pub uri: String,
    pub name: String,
    pub scope: Scope,
    pub version: String,
    pub provenance: Provenance,
    pub policy: Policy,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConceptLabel {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConceptStatus {
    Proposed,
    Active,
    Deprecated,
    Merged,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Concept {
    pub id: ConceptId,
    pub uri: String,
    pub scheme_id: ConceptSchemeId,
    pub pref_label: ConceptLabel,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub alt_labels: Vec<ConceptLabel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notation: Option<String>,
    pub status: ConceptStatus,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConceptRelationKind {
    Broader,
    Narrower,
    Related,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConceptRelation {
    pub id: String,
    pub scheme_id: ConceptSchemeId,
    pub subject_id: ConceptId,
    pub predicate: ConceptRelationKind,
    pub object_id: ConceptId,
    pub provenance: Provenance,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConceptCollection {
    pub id: String,
    pub scheme_id: ConceptSchemeId,
    pub label: ConceptLabel,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub members: Vec<ConceptId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordered: Option<bool>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConceptMappingKind {
    ExactMatch,
    CloseMatch,
    BroadMatch,
    NarrowMatch,
    RelatedMatch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConceptMapping {
    pub id: String,
    pub source_concept_id: ConceptId,
    pub target_scheme_id: ConceptSchemeId,
    pub target_concept_id: ConceptId,
    pub kind: ConceptMappingKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxonomyChangeKind {
    AddConcept,
    UpdateConcept,
    DeprecateConcept,
    MergeConcept,
    AddRelation,
    RemoveRelation,
    AddMapping,
    Restructure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxonomyChangeStatus {
    Proposed,
    Approved,
    Rejected,
    Applied,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyChange {
    pub id: String,
    pub scheme_id: ConceptSchemeId,
    pub kind: TaxonomyChangeKind,
    pub status: TaxonomyChangeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
    pub actor: Actor,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<Timestamp>,
    pub payload: Scalar,
}
