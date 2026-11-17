//! Governed vocabulary records retain inherited ontology and scheme ownership.
use crate::{
    HelixStore,
    error::{Error, Result},
    records::*,
};
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::{OntologyRepository, TaxonomyRepository};
use engram_runtime::CoreResult;
impl HelixStore {
    fn parent(&self, family: &str, id: impl ToString) -> Result<()> {
        self.get::<serde_json::Value>(family, &id.to_string())?
            .ok_or(Error::InvalidConfig(
                "vocabulary parent absent in partition",
            ))?;
        Ok(())
    }
}
#[async_trait]
impl OntologyRepository for HelixStore {
    async fn put_ontology(&self, v: Ontology) -> CoreResult<Ontology> {
        self.check_scope(&v.scope).map_err(core)?;
        self.put(ONTOLOGY, &v).map_err(core)
    }
    async fn put_class(&self, v: OntologyClass) -> CoreResult<OntologyClass> {
        self.atomic(|s| {
            s.parent(ONTOLOGY, &v.ontology_id)?;
            s.put(CLASS, &v)
        })
        .map_err(core)
    }
    async fn put_property(&self, v: OntologyProperty) -> CoreResult<OntologyProperty> {
        self.atomic(|s| {
            s.parent(ONTOLOGY, &v.ontology_id)?;
            s.put(PROPERTY, &v)
        })
        .map_err(core)
    }
    async fn put_axiom(&self, v: OntologyAxiom) -> CoreResult<OntologyAxiom> {
        self.atomic(|s| {
            s.parent(ONTOLOGY, &v.ontology_id)?;
            s.put(AXIOM, &v)
        })
        .map_err(core)
    }
    async fn get_ontology(&self, id: &OntologyId, scope: &Scope) -> CoreResult<Option<Ontology>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(ONTOLOGY, &id.to_string()).map_err(core)
    }
    async fn validate_graph(
        &self,
        graph: &KnowledgeGraphId,
        ontology: &OntologyId,
        scope: &Scope,
    ) -> CoreResult<Vec<OntologyValidationFinding>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        if self
            .get::<Ontology>(ONTOLOGY, &ontology.to_string())
            .map_err(core)?
            .is_none()
        {
            return Ok(vec![]);
        }
        let properties = self
            .filtered::<OntologyProperty>(PROPERTY, "ontologyId", &ontology.to_string())
            .map_err(core)?;
        use engram_knowledge::ontology_validation::{
            VALIDATE_RELATIONSHIP_LIMIT, validate_visible_graph,
        };
        let rows = self
            .scan_window(
                RELATIONSHIP,
                Some(("graphId", &graph.to_string())),
                None,
                Some((0, VALIDATE_RELATIONSHIP_LIMIT + 1)),
            )
            .map_err(core)?
            .into_iter()
            .map(decode::<KnowledgeRelationship>)
            .collect::<Result<Vec<_>>>()
            .map_err(core)?;
        Ok(validate_visible_graph(
            ontology,
            &properties,
            &rows,
            rows.len() > VALIDATE_RELATIONSHIP_LIMIT,
            chrono::Utc::now(),
        ))
    }
}
#[async_trait]
impl TaxonomyRepository for HelixStore {
    async fn put_concept_scheme(&self, v: ConceptScheme) -> CoreResult<ConceptScheme> {
        self.check_scope(&v.scope).map_err(core)?;
        self.put(SCHEME, &v).map_err(core)
    }
    async fn get_concept_scheme(
        &self,
        id: &ConceptSchemeId,
        scope: &Scope,
    ) -> CoreResult<Option<ConceptScheme>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(SCHEME, &id.to_string()).map_err(core)
    }
    async fn put_concept(&self, v: Concept) -> CoreResult<Concept> {
        self.atomic(|s| {
            s.parent(SCHEME, &v.scheme_id)?;
            s.put(CONCEPT, &v)
        })
        .map_err(core)
    }
    async fn put_concept_relation(&self, v: ConceptRelation) -> CoreResult<ConceptRelation> {
        self.atomic(|s| {
            s.parent(SCHEME, &v.scheme_id)?;
            for id in [&v.subject_id, &v.object_id] {
                let c: Concept = s
                    .get(CONCEPT, &id.to_string())?
                    .ok_or(Error::InvalidConfig("concept relation endpoint absent"))?;
                if c.scheme_id != v.scheme_id {
                    return Err(Error::InvalidConfig("concept endpoint scheme mismatch"));
                }
            }
            s.put(CONCEPT_RELATION, &v)
        })
        .map_err(core)
    }
    async fn list_concepts(&self, id: &ConceptSchemeId, scope: &Scope) -> CoreResult<Vec<Concept>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        if self
            .get::<ConceptScheme>(SCHEME, &id.to_string())
            .map_err(core)?
            .is_none()
        {
            return Ok(vec![]);
        }
        self.filtered(CONCEPT, "schemeId", &id.to_string())
            .map_err(core)
    }
}
