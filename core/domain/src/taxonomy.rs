//! Taxonomy contracts for controlled concepts and their evolution.
//!
//! These models are SKOS-aligned where useful, but remain local to engram's
//! contract. Direct relations are stored explicitly; ancestor, descendant, and
//! cross-scheme views should be computed by taxonomy or hierarchy services.

use serde::{Deserialize, Serialize};

use crate::{
    Actor, ConceptId, ConceptSchemeId, EvidenceRef, Metadata, Policy, Provenance, Scalar, Scope,
    Timestamp,
};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxonomyProposalStatus {
    Discovered,
    Proposed,
    Validated,
    Approved,
    Rejected,
    Merged,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyProposal {
    pub id: String,
    pub scheme_id: ConceptSchemeId,
    pub status: TaxonomyProposalStatus,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub changes: Vec<TaxonomyChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<TaxonomyValidationReport>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub semantic_drift: Vec<SemanticDriftFinding>,
    pub proposer: Actor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<Actor>,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxonomyValidationStatus {
    Passed,
    PassedWithWarnings,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxonomyFindingSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyValidationFinding {
    pub id: String,
    pub severity: TaxonomyFindingSeverity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    pub provenance: Provenance,
    pub detected_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyValidationReport {
    pub id: String,
    pub proposal_id: String,
    pub status: TaxonomyValidationStatus,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub findings: Vec<TaxonomyValidationFinding>,
    pub checked_at: Timestamp,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticDriftTargetType {
    Concept,
    ConceptRelation,
    ConceptMapping,
    Scheme,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticDriftSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticDriftFinding {
    pub id: String,
    pub target_type: SemanticDriftTargetType,
    pub target_id: String,
    pub severity: SemanticDriftSeverity,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub evidence: Vec<EvidenceRef>,
    pub detected_at: Timestamp,
}
