//! Knowledge, graph, ontology, and source-ingestion behavior ports.
//!
//! This crate owns source-grounded knowledge contracts that can be backed by
//! document stores, vector indexes, RDF stores, property graphs, or graph
//! databases without depending on memory persistence.

mod taxonomy_validation;

use async_trait::async_trait;
use engram_domain::*;
pub use engram_runtime::{CoreError, CoreResult};

pub use taxonomy_validation::validate_taxonomy_proposal;

/// Persistence port for source-grounded knowledge records.
///
/// Implementations store corpus-derived sources, documents, chunks, entities,
/// and relationships without turning them into agent memories. A backend may be
/// document-oriented, relational, graph-native, or process-local, but it must
/// preserve provenance, policy, and scope so retrieval can compose knowledge
/// with memory safely.
#[async_trait]
pub trait KnowledgeRepository: Send + Sync {
    /// Stores or updates a registered knowledge source.
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource>;

    /// Stores a versioned document extracted from a source.
    async fn put_document(&self, document: SourceDocument) -> CoreResult<SourceDocument>;

    /// Stores the smallest retrievable source-grounded unit.
    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk>;

    /// Looks up a chunk by ID inside the caller-provided scope boundary.
    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>>;

    /// Stores or updates an extracted graph entity.
    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: format!("entity writes are not supported for {}", entity.id),
        })
    }

    /// Stores or updates an extracted graph relationship.
    async fn put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: format!(
                "relationship writes are not supported for {}",
                relationship.id
            ),
        })
    }

    /// Looks up an entity by ID inside the caller-provided scope boundary.
    async fn get_entity(
        &self,
        _id: &EntityId,
        _scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        Ok(None)
    }

    /// Looks up a relationship by ID inside the caller-provided scope boundary.
    async fn get_relationship(
        &self,
        _id: &RelationshipId,
        _scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        Ok(None)
    }

    /// Deletes an entity by ID within the caller-provided scope boundary.
    ///
    /// Returns `true` if a row was deleted, `false` if the entity was not found
    /// or the caller's scope does not match the record's scope (hard delete; no
    /// tombstone). Default implementation returns a not-supported error.
    async fn delete_entity(&self, _id: &EntityId, _scope: &Scope) -> CoreResult<bool> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "entity deletes are not supported".to_owned(),
        })
    }

    /// Deletes a relationship by ID within the caller-provided scope boundary.
    ///
    /// Returns `true` if a row was deleted, `false` if the relationship was not
    /// found or the caller's scope does not match the record's scope (hard delete;
    /// no tombstone). Default implementation returns a not-supported error.
    async fn delete_relationship(&self, _id: &RelationshipId, _scope: &Scope) -> CoreResult<bool> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "relationship deletes are not supported".to_owned(),
        })
    }
}

/// Persistence and traversal port for ontology-backed knowledge graphs.
///
/// This port owns logical graph identity and traversal independent of the
/// physical graph technology. Neo4j labels, RDF triples, SQL joins, or embedded
/// graph indexes are adapter details; callers see scoped graph records and
/// relationship paths with domain provenance.
#[async_trait]
pub trait KnowledgeGraphRepository: Send + Sync {
    /// Stores or updates a graph identity record.
    async fn put_graph(&self, graph: KnowledgeGraph) -> CoreResult<KnowledgeGraph>;

    /// Looks up a graph by ID inside the caller-provided scope boundary.
    async fn get_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeGraph>>;

    /// Returns graph neighbors for a node without crossing scope boundaries.
    async fn neighbors(
        &self,
        graph_id: &KnowledgeGraphId,
        node_id: &EntityId,
        scope: &Scope,
        limit: Option<u32>,
    ) -> CoreResult<Vec<KnowledgeRelationship>>;

    /// Deletes a graph and cascades to every entity and relationship carrying
    /// that `graph_id`, all in a single transaction. Returns `true` if the
    /// graph existed and was deleted. A delete under a non-matching scope is a
    /// no-op returning `false` (hard delete; no tombstone). Default
    /// implementation returns a not-supported error.
    async fn delete_graph(&self, _id: &KnowledgeGraphId, _scope: &Scope) -> CoreResult<bool> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "graph deletes are not supported".to_owned(),
        })
    }

    /// Lists knowledge graphs belonging to `stable_source_key`, visible to
    /// `scope`. Used by the ingest reconciler to find prior graphs for a
    /// `(stable_source_key, path)` pair before writing a replacement.
    ///
    /// Default implementation returns a not-supported error so that a future
    /// adapter that overrides the delete methods but forgets to override this
    /// query fails loudly rather than silently reconciling nothing.
    /// `SqlKnowledgeStore` overrides this — no behavior change on the real path.
    async fn list_graphs_by_source(
        &self,
        _scope: &Scope,
        _stable_source_key: &str,
    ) -> CoreResult<Vec<KnowledgeGraph>> {
        Err(CoreError::Adapter {
            adapter: "knowledge_repository".to_owned(),
            message: "list_graphs_by_source is not supported".to_owned(),
        })
    }
}

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

/// Reads external sources without owning persistence.
///
/// Source readers translate filesystems, Git repositories, URLs, uploads, or
/// APIs into `SourceDocument` records and document content. They should report
/// adapter failures explicitly instead of returning partial reads as complete
/// ingestion.
#[async_trait]
pub trait SourceReader: Send + Sync {
    /// Lists or discovers documents available from a registered source.
    async fn read_source(&self, source: &KnowledgeSource) -> CoreResult<Vec<SourceDocument>>;

    /// Reads extracted textual content for one source document.
    async fn read_document(&self, document: &SourceDocument) -> CoreResult<String>;
}

/// Splits source document content into source-grounded chunks.
///
/// Chunkers preserve enough location and provenance information for later
/// retrieval explanations. Code-aware chunkers should emit symbol or file chunk
/// kinds instead of flattening everything into generic text.
pub trait Chunker: Send + Sync {
    /// Creates retrievable chunks from a document's extracted content.
    fn chunk_document(
        &self,
        source: &KnowledgeSource,
        document: &SourceDocument,
        content: &str,
    ) -> CoreResult<Vec<KnowledgeChunk>>;
}

/// Coordinates source reading, chunking, and knowledge persistence.
///
/// Ingestion services assemble source readers, chunkers, and repositories into
/// an idempotent source-to-knowledge pipeline. Dry runs should compute planned
/// writes without persisting sources, documents, chunks, entities, or graph
/// relationships.
#[async_trait]
pub trait IngestionService: Send + Sync {
    /// Ingests a registered source and returns chunks written or planned.
    async fn ingest(&self, request: IngestRequest) -> CoreResult<Vec<KnowledgeChunk>>;
}
