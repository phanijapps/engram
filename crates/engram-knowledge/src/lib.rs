//! Knowledge, graph, ontology, and source-ingestion behavior ports.
//!
//! This crate owns source-grounded knowledge contracts that can be backed by
//! document stores, vector indexes, RDF stores, property graphs, or graph
//! databases without depending on memory persistence.

use async_trait::async_trait;
use engram_domain::*;
pub use engram_runtime::{CoreError, CoreResult};

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
