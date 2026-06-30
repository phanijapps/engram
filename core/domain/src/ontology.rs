//! Ontology contracts for knowledge graph governance.
//!
//! Ontologies describe the allowed classes, properties, axioms, and validation
//! constraints for knowledge graphs. The model is storage-neutral: adapters may
//! project it to property graphs, RDF/OWL, SHACL, relational tables, or another
//! graph technology.

use serde::{Deserialize, Serialize};

use crate::{
    ConceptRef, EntityRef, Metadata, OntologyAxiomId, OntologyClassId, OntologyId,
    OntologyPropertyId, Policy, Provenance, Scalar, Scope, Timestamp,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyLanguage {
    PropertyGraph,
    Rdf,
    Rdfs,
    Owl,
    Shacl,
    Skos,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyStatus {
    Draft,
    Active,
    Deprecated,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyImport {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ontology {
    pub id: OntologyId,
    pub uri: String,
    pub name: String,
    pub scope: Scope,
    pub language: OntologyLanguage,
    pub version: String,
    pub status: OntologyStatus,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub imports: Vec<OntologyImport>,
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
pub enum OntologyTermStatus {
    Proposed,
    Active,
    Deprecated,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyClass {
    pub id: OntologyClassId,
    pub ontology_id: OntologyId,
    pub uri: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub parent_class_ids: Vec<OntologyClassId>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub concept_refs: Vec<ConceptRef>,
    pub status: OntologyTermStatus,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyPropertyKind {
    Object,
    Data,
    Annotation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyProperty {
    pub id: OntologyPropertyId,
    pub ontology_id: OntologyId,
    pub uri: String,
    pub label: String,
    pub kind: OntologyPropertyKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_class_id: Option<OntologyClassId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_class_id: Option<OntologyClassId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datatype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inverse_property_id: Option<OntologyPropertyId>,
    pub status: OntologyTermStatus,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyAxiomKind {
    SubClassOf,
    EquivalentClass,
    DisjointWith,
    Domain,
    Range,
    Functional,
    InverseOf,
    Transitive,
    Symmetric,
    Cardinality,
    Constraint,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyAxiom {
    pub id: OntologyAxiomId,
    pub ontology_id: OntologyId,
    pub kind: OntologyAxiomKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_class_id: Option<OntologyClassId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_id: Option<OntologyPropertyId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_class_id: Option<OntologyClassId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<Scalar>,
    pub provenance: Provenance,
    pub created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyValidationSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyValidationFinding {
    pub id: String,
    pub ontology_id: OntologyId,
    pub severity: OntologyValidationSeverity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<EntityRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axiom_id: Option<OntologyAxiomId>,
    pub provenance: Provenance,
    pub detected_at: Timestamp,
}
