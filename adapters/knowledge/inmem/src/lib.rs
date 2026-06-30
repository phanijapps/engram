//! In-memory adapter for source-grounded knowledge tests.
//!
//! This crate is intentionally process-local and test-oriented. It proves the
//! `engram-knowledge` repository, graph, and ontology ports without making the
//! memory test adapter own graph storage. Durable graph, document, RDF, or
//! Neo4j-backed implementations belong in separate adapter crates.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository};
use engram_runtime::{CoreError, CoreResult};

#[derive(Debug, Default)]
struct InMemoryKnowledgeState {
    sources: BTreeMap<String, KnowledgeSource>,
    documents: BTreeMap<String, SourceDocument>,
    chunks: BTreeMap<String, KnowledgeChunk>,
    entities: BTreeMap<String, KnowledgeEntity>,
    relationships: BTreeMap<String, KnowledgeRelationship>,
    graphs: BTreeMap<String, KnowledgeGraph>,
    ontologies: BTreeMap<String, Ontology>,
    classes: BTreeMap<String, OntologyClass>,
    properties: BTreeMap<String, OntologyProperty>,
    axioms: BTreeMap<String, OntologyAxiom>,
}

/// Process-local knowledge adapter for conformance tests and examples.
///
/// The store keeps source/document/chunk, graph, and ontology records in
/// separate maps so tests can verify each knowledge boundary independently. It
/// is not a production cache and makes no durability or concurrency guarantees
/// beyond process-local mutex protection.
#[derive(Debug, Clone, Default)]
pub struct InMemoryKnowledgeStore {
    state: Arc<Mutex<InMemoryKnowledgeState>>,
}

impl InMemoryKnowledgeStore {
    /// Creates an empty process-local knowledge store for graph, ontology, and
    /// source-ingestion conformance tests without sharing memory adapter state.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_state(&self) -> CoreResult<MutexGuard<'_, InMemoryKnowledgeState>> {
        self.state.lock().map_err(|_| CoreError::Adapter {
            adapter: "in_memory_knowledge".to_owned(),
            message: "state lock poisoned".to_owned(),
        })
    }
}

#[async_trait]
impl KnowledgeRepository for InMemoryKnowledgeStore {
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        let mut state = self.lock_state()?;
        state.sources.insert(source.id.to_string(), source.clone());
        Ok(source)
    }

    async fn put_document(&self, document: SourceDocument) -> CoreResult<SourceDocument> {
        let mut state = self.lock_state()?;
        state
            .documents
            .insert(document.id.to_string(), document.clone());
        Ok(document)
    }

    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        let mut state = self.lock_state()?;
        state.chunks.insert(chunk.id.to_string(), chunk.clone());
        Ok(chunk)
    }

    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>> {
        let state = self.lock_state()?;
        let Some(chunk) = state.chunks.get(id.as_str()) else {
            return Ok(None);
        };
        let Some(document) = state.documents.get(chunk.document_id.as_str()) else {
            return Ok(None);
        };
        let Some(source) = state.sources.get(document.source_id.as_str()) else {
            return Ok(None);
        };
        Ok(scope_allows(&source.scope, scope).then(|| chunk.clone()))
    }

    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        let mut state = self.lock_state()?;
        state.entities.insert(entity.id.to_string(), entity.clone());
        Ok(entity)
    }

    async fn put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        let mut state = self.lock_state()?;
        state
            .relationships
            .insert(relationship.id.to_string(), relationship.clone());
        Ok(relationship)
    }

    async fn get_entity(
        &self,
        id: &EntityId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        let state = self.lock_state()?;
        let Some(entity) = state.entities.get(id.as_str()) else {
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
        let Some(relationship) = state.relationships.get(id.as_str()) else {
            return Ok(None);
        };
        Ok(scope_allows(&relationship.scope, scope).then(|| relationship.clone()))
    }
}

#[async_trait]
impl KnowledgeGraphRepository for InMemoryKnowledgeStore {
    async fn put_graph(&self, graph: KnowledgeGraph) -> CoreResult<KnowledgeGraph> {
        let mut state = self.lock_state()?;
        state.graphs.insert(graph.id.to_string(), graph.clone());
        Ok(graph)
    }

    async fn get_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeGraph>> {
        let state = self.lock_state()?;
        let Some(graph) = state.graphs.get(id.as_str()) else {
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
            .graphs
            .get(graph_id.as_str())
            .ok_or_else(|| CoreError::NotFound {
                target_type: "knowledge_graph",
                target_id: graph_id.to_string(),
            })?;
        if !scope_allows(&graph.scope, scope) {
            return Ok(Vec::new());
        }

        let mut relationships = state
            .relationships
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
impl OntologyRepository for InMemoryKnowledgeStore {
    async fn put_ontology(&self, ontology: Ontology) -> CoreResult<Ontology> {
        let mut state = self.lock_state()?;
        state
            .ontologies
            .insert(ontology.id.to_string(), ontology.clone());
        Ok(ontology)
    }

    async fn put_class(&self, class: OntologyClass) -> CoreResult<OntologyClass> {
        let mut state = self.lock_state()?;
        state.classes.insert(class.id.to_string(), class.clone());
        Ok(class)
    }

    async fn put_property(&self, property: OntologyProperty) -> CoreResult<OntologyProperty> {
        let mut state = self.lock_state()?;
        state
            .properties
            .insert(property.id.to_string(), property.clone());
        Ok(property)
    }

    async fn put_axiom(&self, axiom: OntologyAxiom) -> CoreResult<OntologyAxiom> {
        let mut state = self.lock_state()?;
        state.axioms.insert(axiom.id.to_string(), axiom.clone());
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
            .graphs
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

fn scope_allows(record_scope: &Scope, request_scope: &Scope) -> bool {
    record_scope.tenant == request_scope.tenant
        && optional_scope_matches(&record_scope.subject, &request_scope.subject)
        && optional_scope_matches(&record_scope.workspace, &request_scope.workspace)
        && optional_scope_matches(&record_scope.session, &request_scope.session)
        && optional_scope_matches(&record_scope.environment, &request_scope.environment)
}

fn optional_scope_matches(record_value: &Option<String>, request_value: &Option<String>) -> bool {
    request_value
        .as_ref()
        .is_none_or(|value| record_value.as_ref() == Some(value))
}
