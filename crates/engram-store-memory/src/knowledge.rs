//! In-memory knowledge repository implementation.
//!
//! Knowledge records share the process-local adapter for tests, but stay in
//! separate maps from memories and events so source-grounded content does not
//! become agent memory by accident.

use async_trait::async_trait;
use engram_core::{
    CoreError, CoreResult, KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository,
};
use engram_domain::*;

use crate::{scope::scope_allows, service::InMemoryMemoryService};

#[async_trait]
impl KnowledgeRepository for InMemoryMemoryService {
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        let mut state = self.lock_state()?;
        state
            .knowledge_sources
            .insert(source.id.to_string(), source.clone());
        Ok(source)
    }

    async fn put_document(&self, document: SourceDocument) -> CoreResult<SourceDocument> {
        let mut state = self.lock_state()?;
        state
            .source_documents
            .insert(document.id.to_string(), document.clone());
        Ok(document)
    }

    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        let mut state = self.lock_state()?;
        state
            .knowledge_chunks
            .insert(chunk.id.to_string(), chunk.clone());
        Ok(chunk)
    }

    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>> {
        let state = self.lock_state()?;
        let Some(chunk) = state.knowledge_chunks.get(id.as_str()) else {
            return Ok(None);
        };
        let Some(document) = state.source_documents.get(chunk.document_id.as_str()) else {
            return Ok(None);
        };
        let Some(source) = state.knowledge_sources.get(document.source_id.as_str()) else {
            return Ok(None);
        };
        Ok(scope_allows(&source.scope, scope).then(|| chunk.clone()))
    }

    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        let mut state = self.lock_state()?;
        state
            .knowledge_entities
            .insert(entity.id.to_string(), entity.clone());
        Ok(entity)
    }

    async fn put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        let mut state = self.lock_state()?;
        state
            .knowledge_relationships
            .insert(relationship.id.to_string(), relationship.clone());
        Ok(relationship)
    }

    async fn get_entity(
        &self,
        id: &EntityId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        let state = self.lock_state()?;
        let Some(entity) = state.knowledge_entities.get(id.as_str()) else {
            return Ok(None);
        };
        Ok(scope_allows(&entity.scope, scope).then(|| entity.clone()))
    }

    async fn get_relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        let state = self.lock_state()?;
        let Some(relationship) = state.knowledge_relationships.get(id.as_str()) else {
            return Ok(None);
        };
        Ok(scope_allows(&relationship.scope, scope).then(|| relationship.clone()))
    }
}

#[async_trait]
impl KnowledgeGraphRepository for InMemoryMemoryService {
    async fn put_graph(&self, graph: KnowledgeGraph) -> CoreResult<KnowledgeGraph> {
        let mut state = self.lock_state()?;
        state
            .knowledge_graphs
            .insert(graph.id.to_string(), graph.clone());
        Ok(graph)
    }

    async fn get_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeGraph>> {
        let state = self.lock_state()?;
        let Some(graph) = state.knowledge_graphs.get(id.as_str()) else {
            return Ok(None);
        };
        Ok(scope_allows(&graph.scope, scope).then(|| graph.clone()))
    }

    async fn neighbors(
        &self,
        graph_id: &KnowledgeGraphId,
        node_id: &EntityId,
        scope: &Scope,
        limit: Option<u32>,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        let state = self.lock_state()?;
        let graph = state
            .knowledge_graphs
            .get(graph_id.as_str())
            .ok_or_else(|| CoreError::NotFound {
                target_type: "knowledge_graph",
                target_id: graph_id.to_string(),
            })?;
        if !scope_allows(&graph.scope, scope) {
            return Ok(Vec::new());
        }

        let mut relationships = state
            .knowledge_relationships
            .values()
            .filter(|relationship| {
                relationship.graph_id.as_ref() == Some(graph_id)
                    && scope_allows(&relationship.scope, scope)
                    && relationship.subject.id.as_ref() == Some(node_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        relationships.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        if let Some(limit) = limit {
            relationships.truncate(limit as usize);
        }
        Ok(relationships)
    }
}

#[async_trait]
impl OntologyRepository for InMemoryMemoryService {
    async fn put_ontology(&self, ontology: Ontology) -> CoreResult<Ontology> {
        let mut state = self.lock_state()?;
        state
            .ontologies
            .insert(ontology.id.to_string(), ontology.clone());
        Ok(ontology)
    }

    async fn put_class(&self, class: OntologyClass) -> CoreResult<OntologyClass> {
        let mut state = self.lock_state()?;
        state
            .ontology_classes
            .insert(class.id.to_string(), class.clone());
        Ok(class)
    }

    async fn put_property(&self, property: OntologyProperty) -> CoreResult<OntologyProperty> {
        let mut state = self.lock_state()?;
        state
            .ontology_properties
            .insert(property.id.to_string(), property.clone());
        Ok(property)
    }

    async fn put_axiom(&self, axiom: OntologyAxiom) -> CoreResult<OntologyAxiom> {
        let mut state = self.lock_state()?;
        state
            .ontology_axioms
            .insert(axiom.id.to_string(), axiom.clone());
        Ok(axiom)
    }

    async fn get_ontology(&self, id: &OntologyId, scope: &Scope) -> CoreResult<Option<Ontology>> {
        let state = self.lock_state()?;
        let Some(ontology) = state.ontologies.get(id.as_str()) else {
            return Ok(None);
        };
        Ok(scope_allows(&ontology.scope, scope).then(|| ontology.clone()))
    }

    async fn validate_graph(
        &self,
        graph_id: &KnowledgeGraphId,
        ontology_id: &OntologyId,
        scope: &Scope,
    ) -> CoreResult<Vec<OntologyValidationFinding>> {
        let state = self.lock_state()?;
        let graph_visible = state
            .knowledge_graphs
            .get(graph_id.as_str())
            .is_some_and(|graph| scope_allows(&graph.scope, scope));
        let ontology_visible = state
            .ontologies
            .get(ontology_id.as_str())
            .is_some_and(|ontology| scope_allows(&ontology.scope, scope));
        if graph_visible && ontology_visible {
            Ok(Vec::new())
        } else {
            Err(CoreError::NotFound {
                target_type: "graph_or_ontology",
                target_id: format!("{graph_id}:{ontology_id}"),
            })
        }
    }
}
