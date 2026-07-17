//! Surreal knowledge cell — the 4 knowledge ports over embedded SurrealKV.
//!
//! Mirrors `engram-store-sqlite::knowledge`: sources/documents/chunks,
//! entities/relationships/graphs, concept schemes/concepts/relations, and
//! ontology records persisted as DTOs under a `data` field with scope filtering.
//! Chunk/document visibility inherits from their owning source (same rule as the
//! SQLite adapter). `neighbors` loads a graph's relationships and filters to the
//! node's adjacent edges.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
};
use engram_runtime::CoreResult;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::SurrealConnection;
use crate::util::{DataWrapper, scope_allows, surreal_err};

const SOURCE_TABLE: &str = "knowledge_source";
const DOCUMENT_TABLE: &str = "knowledge_document";
const CHUNK_TABLE: &str = "knowledge_chunk";
const ENTITY_TABLE: &str = "knowledge_entity";
const RELATIONSHIP_TABLE: &str = "knowledge_relationship";
const GRAPH_TABLE: &str = "knowledge_graph";
const SCHEME_TABLE: &str = "concept_scheme";
const CONCEPT_TABLE: &str = "concept";
const CONCEPT_RELATION_TABLE: &str = "concept_relation";
const ONTOLOGY_TABLE: &str = "ontology";
const ONTOLOGY_CLASS_TABLE: &str = "ontology_class";
const ONTOLOGY_PROPERTY_TABLE: &str = "ontology_property";
const ONTOLOGY_AXIOM_TABLE: &str = "ontology_axiom";

/// The 4 knowledge ports (`KnowledgeRepository` + `KnowledgeGraphRepository` +
/// `TaxonomyRepository` + `OntologyRepository`) backed by embedded SurrealKV.
pub struct SurrealKnowledgeStore {
    conn: Arc<SurrealConnection>,
}

impl SurrealKnowledgeStore {
    pub fn new(conn: Arc<SurrealConnection>) -> Self {
        Self { conn }
    }

    async fn put_record<T: Serialize + Clone + 'static>(
        &self,
        table: &str,
        key: String,
        record: &T,
    ) -> CoreResult<()> {
        let db = self.conn.db().await?;
        db.query(&format!(
            "UPSERT type::thing('{table}', $key) SET data = $record"
        ))
        .bind(("key", key))
        .bind(("record", record.clone()))
        .await
        .map_err(surreal_err)?;
        Ok(())
    }

    async fn get_record<T: DeserializeOwned + 'static>(
        &self,
        table: &str,
        key: &str,
    ) -> CoreResult<Option<T>> {
        let db = self.conn.db().await?;
        let mut res = db
            .query(&format!("SELECT data FROM type::thing('{table}', $key)"))
            .bind(("key", key.to_string()))
            .await
            .map_err(surreal_err)?;
        let rows: Vec<DataWrapper<T>> = res.take(0).map_err(surreal_err)?;
        Ok(rows.into_iter().next().map(|w| w.data))
    }

    async fn list_records<T: DeserializeOwned + 'static>(&self, table: &str) -> CoreResult<Vec<T>> {
        let db = self.conn.db().await?;
        let mut res = db
            .query(&format!("SELECT data FROM {table}"))
            .await
            .map_err(surreal_err)?;
        let rows: Vec<DataWrapper<T>> = res.take(0).map_err(surreal_err)?;
        Ok(rows.into_iter().map(|w| w.data).collect())
    }
}

#[async_trait]
impl KnowledgeRepository for SurrealKnowledgeStore {
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        self.put_record(SOURCE_TABLE, source.id.to_string(), &source)
            .await?;
        Ok(source)
    }

    async fn put_document(&self, document: SourceDocument) -> CoreResult<SourceDocument> {
        self.put_record(DOCUMENT_TABLE, document.id.to_string(), &document)
            .await?;
        Ok(document)
    }

    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        self.put_record(CHUNK_TABLE, chunk.id.to_string(), &chunk)
            .await?;
        Ok(chunk)
    }

    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>> {
        // Chunk visibility inherits from its owning source (chunk -> document ->
        // source), mirroring the SQLite adapter.
        let Some(chunk) = self
            .get_record::<KnowledgeChunk>(CHUNK_TABLE, &id.to_string())
            .await?
        else {
            return Ok(None);
        };
        let doc = self
            .get_record::<SourceDocument>(DOCUMENT_TABLE, &chunk.document_id.to_string())
            .await?;
        let Some(document) = doc else {
            return Ok(None);
        };
        let source = self
            .get_record::<KnowledgeSource>(SOURCE_TABLE, &document.source_id.to_string())
            .await?;
        Ok(source
            .filter(|s| scope_allows(&s.scope, scope))
            .map(|_| chunk))
    }

    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        self.put_record(ENTITY_TABLE, entity.id.to_string(), &entity)
            .await?;
        Ok(entity)
    }

    async fn put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        self.put_record(
            RELATIONSHIP_TABLE,
            relationship.id.to_string(),
            &relationship,
        )
        .await?;
        Ok(relationship)
    }

    async fn get_entity(
        &self,
        id: &EntityId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        Ok(self
            .get_record::<KnowledgeEntity>(ENTITY_TABLE, &id.to_string())
            .await?
            .filter(|e| scope_allows(&e.scope, scope)))
    }

    async fn get_relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        Ok(self
            .get_record::<KnowledgeRelationship>(RELATIONSHIP_TABLE, &id.to_string())
            .await?
            .filter(|r| scope_allows(&r.scope, scope)))
    }
}

#[async_trait]
impl KnowledgeGraphRepository for SurrealKnowledgeStore {
    async fn put_graph(&self, graph: KnowledgeGraph) -> CoreResult<KnowledgeGraph> {
        self.put_record(GRAPH_TABLE, graph.id.to_string(), &graph)
            .await?;
        Ok(graph)
    }

    async fn get_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeGraph>> {
        Ok(self
            .get_record::<KnowledgeGraph>(GRAPH_TABLE, &id.to_string())
            .await?
            .filter(|g| scope_allows(&g.scope, scope)))
    }

    async fn neighbors(
        &self,
        graph_id: &KnowledgeGraphId,
        node_id: &EntityId,
        scope: &Scope,
        limit: Option<u32>,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        let mut edges: Vec<KnowledgeRelationship> = self
            .list_records::<KnowledgeRelationship>(RELATIONSHIP_TABLE)
            .await?
            .into_iter()
            .filter(|r| scope_allows(&r.scope, scope))
            .filter(|r| r.graph_id.as_ref() == Some(graph_id))
            .filter(|r| {
                r.subject.id.as_ref() == Some(node_id) || r.object.id.as_ref() == Some(node_id)
            })
            .collect();
        if let Some(limit) = limit {
            edges.truncate(limit as usize);
        }
        Ok(edges)
    }
}

#[async_trait]
impl TaxonomyRepository for SurrealKnowledgeStore {
    async fn put_concept_scheme(&self, scheme: ConceptScheme) -> CoreResult<ConceptScheme> {
        self.put_record(SCHEME_TABLE, scheme.id.to_string(), &scheme)
            .await?;
        Ok(scheme)
    }

    async fn get_concept_scheme(
        &self,
        id: &ConceptSchemeId,
        scope: &Scope,
    ) -> CoreResult<Option<ConceptScheme>> {
        Ok(self
            .get_record::<ConceptScheme>(SCHEME_TABLE, &id.to_string())
            .await?
            .filter(|s| scope_allows(&s.scope, scope)))
    }

    async fn put_concept(&self, concept: Concept) -> CoreResult<Concept> {
        self.put_record(CONCEPT_TABLE, concept.id.to_string(), &concept)
            .await?;
        Ok(concept)
    }

    async fn put_concept_relation(&self, relation: ConceptRelation) -> CoreResult<ConceptRelation> {
        self.put_record(CONCEPT_RELATION_TABLE, relation.id.to_string(), &relation)
            .await?;
        Ok(relation)
    }

    async fn list_concepts(
        &self,
        scheme_id: &ConceptSchemeId,
        scope: &Scope,
    ) -> CoreResult<Vec<Concept>> {
        // Concepts inherit scope from their owning scheme: return them only when
        // the caller can see the scheme.
        if self.get_concept_scheme(scheme_id, scope).await?.is_none() {
            return Ok(Vec::new());
        }
        Ok(self
            .list_records::<Concept>(CONCEPT_TABLE)
            .await?
            .into_iter()
            .filter(|c| c.scheme_id == *scheme_id)
            .collect())
    }
}

#[async_trait]
impl OntologyRepository for SurrealKnowledgeStore {
    async fn put_ontology(&self, ontology: Ontology) -> CoreResult<Ontology> {
        self.put_record(ONTOLOGY_TABLE, ontology.id.to_string(), &ontology)
            .await?;
        Ok(ontology)
    }

    async fn put_class(&self, class: OntologyClass) -> CoreResult<OntologyClass> {
        self.put_record(ONTOLOGY_CLASS_TABLE, class.id.to_string(), &class)
            .await?;
        Ok(class)
    }

    async fn put_property(&self, property: OntologyProperty) -> CoreResult<OntologyProperty> {
        self.put_record(ONTOLOGY_PROPERTY_TABLE, property.id.to_string(), &property)
            .await?;
        Ok(property)
    }

    async fn put_axiom(&self, axiom: OntologyAxiom) -> CoreResult<OntologyAxiom> {
        self.put_record(ONTOLOGY_AXIOM_TABLE, axiom.id.to_string(), &axiom)
            .await?;
        Ok(axiom)
    }

    async fn get_ontology(&self, id: &OntologyId, scope: &Scope) -> CoreResult<Option<Ontology>> {
        Ok(self
            .get_record::<Ontology>(ONTOLOGY_TABLE, &id.to_string())
            .await?
            .filter(|o| scope_allows(&o.scope, scope)))
    }

    async fn validate_graph(
        &self,
        _graph_id: &KnowledgeGraphId,
        _ontology_id: &OntologyId,
        _scope: &Scope,
    ) -> CoreResult<Vec<OntologyValidationFinding>> {
        // Advisory ontology validation. v1 returns no findings (the rule engine
        // is advisory and out of scope for the storage cell); the SQLite adapter's
        // full validator can be ported later if required.
        Ok(Vec::new())
    }
}
