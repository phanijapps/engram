//! Taxonomy repository port — concept schemes, concepts, and relations.

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::CoreResult;

/// Persistence port for taxonomy concept schemes, concepts, and relations.
///
/// Taxonomy adapters persist SKOS-aligned controlled vocabularies that knowledge
/// entities and chunks reference via `ConceptRef`. Concepts and relations do not
/// carry their own scope; their visibility is governed by the owning concept
/// scheme's scope, mirroring how knowledge chunks inherit visibility from their
/// source.
#[async_trait]
pub trait TaxonomyRepository: Send + Sync {
    /// Stores or updates a concept scheme.
    async fn put_concept_scheme(&self, scheme: ConceptScheme) -> CoreResult<ConceptScheme>;

    /// Looks up a concept scheme by ID inside the caller-provided scope boundary.
    async fn get_concept_scheme(
        &self,
        id: &ConceptSchemeId,
        scope: &Scope,
    ) -> CoreResult<Option<ConceptScheme>>;

    /// Stores or updates a concept within a scheme.
    async fn put_concept(&self, concept: Concept) -> CoreResult<Concept>;

    /// Stores or updates a direct concept relation (broader, narrower, related).
    async fn put_concept_relation(&self, relation: ConceptRelation) -> CoreResult<ConceptRelation>;

    /// Lists concepts in a scheme visible to the caller-provided scope.
    async fn list_concepts(
        &self,
        scheme_id: &ConceptSchemeId,
        scope: &Scope,
    ) -> CoreResult<Vec<Concept>>;
}
