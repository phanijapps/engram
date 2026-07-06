//! Ontology repository port — graph vocabulary persistence and validation.

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::CoreResult;

/// Persistence and validation port for graph ontologies.
///
/// Ontology adapters persist the vocabulary that governs graph entities and
/// relationships: classes, properties, axioms, imports, and validation findings.
/// Validation is advisory unless an adapter or policy chooses to reject writes
/// based on returned findings.
#[async_trait]
pub trait OntologyRepository: Send + Sync {
    /// Stores or updates an ontology identity record.
    async fn put_ontology(&self, ontology: Ontology) -> CoreResult<Ontology>;

    /// Stores or updates an ontology class.
    async fn put_class(&self, class: OntologyClass) -> CoreResult<OntologyClass>;

    /// Stores or updates an ontology property.
    async fn put_property(&self, property: OntologyProperty) -> CoreResult<OntologyProperty>;

    /// Stores or updates an ontology axiom or constraint.
    async fn put_axiom(&self, axiom: OntologyAxiom) -> CoreResult<OntologyAxiom>;

    /// Looks up an ontology by ID inside the caller-provided scope boundary.
    async fn get_ontology(&self, id: &OntologyId, scope: &Scope) -> CoreResult<Option<Ontology>>;

    /// Validates graph records against the ontology constraints visible to scope.
    async fn validate_graph(
        &self,
        graph_id: &KnowledgeGraphId,
        ontology_id: &OntologyId,
        scope: &Scope,
    ) -> CoreResult<Vec<OntologyValidationFinding>>;
}
