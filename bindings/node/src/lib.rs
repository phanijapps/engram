//! Node-API bridge for Engram memory operations.
//!
//! The binding is intentionally a JSON transport over Rust behavior. TypeScript
//! packages own ergonomics; this crate owns serialization round trips into the
//! Rust memory service.

use engram_domain::{
    Concept, ConceptRelation, ConceptScheme, Id, KnowledgeChunk, KnowledgeEntity, KnowledgeGraph,
    KnowledgeRelationship, KnowledgeSource, Ontology, OntologyAxiom, OntologyClass,
    OntologyProperty, Scope, SourceDocument, SourceDocumentKind,
};
use engram_domain::{ForgetRequest, RetrievalRequest, WriteMemoryRequest};
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, GraphExtractor, KnowledgeIngestor, PlainTextChunker,
    PlainTextChunkerOptions,
};
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
};
use engram_memory::{CoreError, MemoryService};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use engram_store_sql::SqlMemoryService;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::Serialize;

/// Stateful local memory engine exposed to Node through N-API.
///
/// Each instance owns one SQLite-backed Rust service so write, retrieve, and
/// forget calls observe the same local state without TypeScript reimplementing
/// memory behavior.
#[napi]
pub struct NativeMemoryEngine {
    service: SqlMemoryService,
}

#[napi]
impl NativeMemoryEngine {
    /// Opens a local in-memory SQLite engine for Node consumers and tests.
    ///
    /// The database is process-local to the native engine instance. Durable
    /// file-backed configuration should be added through explicit adapter
    /// options rather than inferred from JavaScript global state.
    #[napi(constructor)]
    pub fn new(path: Option<String>) -> Result<Self> {
        let service = match path {
            Some(path) => SqlMemoryService::open_file(path),
            None => SqlMemoryService::open_in_memory(),
        }
        .map_err(to_napi_error)?;
        Ok(Self { service })
    }

    /// Writes one memory using a JSON-encoded v1 `WriteMemoryRequest`.
    ///
    /// The returned string is a JSON-encoded v1 `WriteMemoryResponse` produced
    /// by Rust service behavior.
    #[napi(js_name = "writeMemoryJson")]
    pub fn write_memory_json(&self, request_json: String) -> Result<String> {
        let request = decode::<WriteMemoryRequest>(&request_json)?;
        let response = block_on(self.service.write_memory(request)).map_err(to_napi_error)?;
        encode(&response)
    }

    /// Retrieves context using a JSON-encoded v1 `RetrievalRequest`.
    ///
    /// The binding returns the Rust service response unchanged as JSON so the
    /// TypeScript client can validate and compose it without reimplementing
    /// retrieval behavior.
    #[napi(js_name = "retrieveJson")]
    pub fn retrieve_json(&self, request_json: String) -> Result<String> {
        let request = decode::<RetrievalRequest>(&request_json)?;
        let response = block_on(self.service.retrieve(request)).map_err(to_napi_error)?;
        encode(&response)
    }

    /// Applies a forget operation using a JSON-encoded v1 `ForgetRequest`.
    ///
    /// Policy, scope, lifecycle status, and audit-event semantics are enforced
    /// by the Rust service behind the binding.
    #[napi(js_name = "forgetJson")]
    pub fn forget_json(&self, request_json: String) -> Result<String> {
        let request = decode::<ForgetRequest>(&request_json)?;
        let response = block_on(self.service.forget(request)).map_err(to_napi_error)?;
        encode(&response)
    }
}

/// Stateful local knowledge + taxonomy engine exposed to Node through N-API.
///
/// Owns one SQLite-backed `SqlKnowledgeStore` so graph, source, and taxonomy
/// calls observe the same scoped state. The methods are JSON transports over
/// the `engram-knowledge` ports; TypeScript owns ergonomics.
#[napi]
pub struct NativeKnowledgeEngine {
    store: SqlKnowledgeStore,
}

#[napi]
impl NativeKnowledgeEngine {
    /// Opens a SQLite knowledge engine. Pass a path for a durable file-backed
    /// store (shared with other engines that use the same file); omit for
    /// in-memory.
    #[napi(constructor)]
    pub fn new(path: Option<String>) -> Result<Self> {
        let store = match path {
            Some(path) => SqlKnowledgeStore::open_file(path),
            None => SqlKnowledgeStore::open_in_memory(),
        }
        .map_err(to_napi_error)?;
        Ok(Self { store })
    }

    // --- KnowledgeRepository -------------------------------------------------

    #[napi(js_name = "putSourceJson")]
    pub fn put_source_json(&self, source_json: String) -> Result<String> {
        let source: KnowledgeSource = decode(&source_json)?;
        let result = block_on(self.store.put_source(source)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "putDocumentJson")]
    pub fn put_document_json(&self, document_json: String) -> Result<String> {
        let document: SourceDocument = decode(&document_json)?;
        let result = block_on(self.store.put_document(document)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "putChunkJson")]
    pub fn put_chunk_json(&self, chunk_json: String) -> Result<String> {
        let chunk: KnowledgeChunk = decode(&chunk_json)?;
        let result = block_on(self.store.put_chunk(chunk)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "getChunkJson")]
    pub fn get_chunk_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.get_chunk(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "putEntityJson")]
    pub fn put_entity_json(&self, entity_json: String) -> Result<String> {
        let entity: KnowledgeEntity = decode(&entity_json)?;
        let result = block_on(self.store.put_entity(entity)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "putRelationshipJson")]
    pub fn put_relationship_json(&self, relationship_json: String) -> Result<String> {
        let relationship: KnowledgeRelationship = decode(&relationship_json)?;
        let result = block_on(self.store.put_relationship(relationship)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "getEntityJson")]
    pub fn get_entity_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.get_entity(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "getRelationshipJson")]
    pub fn get_relationship_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.get_relationship(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    // --- KnowledgeGraphRepository --------------------------------------------

    #[napi(js_name = "putGraphJson")]
    pub fn put_graph_json(&self, graph_json: String) -> Result<String> {
        let graph: KnowledgeGraph = decode(&graph_json)?;
        let result = block_on(self.store.put_graph(graph)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "getGraphJson")]
    pub fn get_graph_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.get_graph(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "neighborsJson")]
    pub fn neighbors_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let graph_id = id_field(&value, "graphId")?;
        let node_id = id_field(&value, "nodeId")?;
        let scope = scope_field(&value)?;
        let limit = value
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        let result = block_on(self.store.neighbors(&graph_id, &node_id, &scope, limit))
            .map_err(to_napi_error)?;
        encode(&result)
    }

    // --- TaxonomyRepository --------------------------------------------------

    #[napi(js_name = "putConceptSchemeJson")]
    pub fn put_concept_scheme_json(&self, scheme_json: String) -> Result<String> {
        let scheme: ConceptScheme = decode(&scheme_json)?;
        let result = block_on(self.store.put_concept_scheme(scheme)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "getConceptSchemeJson")]
    pub fn get_concept_scheme_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.get_concept_scheme(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "putConceptJson")]
    pub fn put_concept_json(&self, concept_json: String) -> Result<String> {
        let concept: Concept = decode(&concept_json)?;
        let result = block_on(self.store.put_concept(concept)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "putConceptRelationJson")]
    pub fn put_concept_relation_json(&self, relation_json: String) -> Result<String> {
        let relation: ConceptRelation = decode(&relation_json)?;
        let result = block_on(self.store.put_concept_relation(relation)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "listConceptsJson")]
    pub fn list_concepts_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scheme_id = id_field(&value, "schemeId")?;
        let scope = scope_field(&value)?;
        let result =
            block_on(self.store.list_concepts(&scheme_id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    // --- OntologyRepository --------------------------------------------------

    #[napi(js_name = "putOntologyJson")]
    pub fn put_ontology_json(&self, ontology_json: String) -> Result<String> {
        let ontology: Ontology = decode(&ontology_json)?;
        let result = block_on(self.store.put_ontology(ontology)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "getOntologyJson")]
    pub fn get_ontology_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.get_ontology(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "putClassJson")]
    pub fn put_class_json(&self, class_json: String) -> Result<String> {
        let class: OntologyClass = decode(&class_json)?;
        let result = block_on(self.store.put_class(class)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "putPropertyJson")]
    pub fn put_property_json(&self, property_json: String) -> Result<String> {
        let property: OntologyProperty = decode(&property_json)?;
        let result = block_on(self.store.put_property(property)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "putAxiomJson")]
    pub fn put_axiom_json(&self, axiom_json: String) -> Result<String> {
        let axiom: OntologyAxiom = decode(&axiom_json)?;
        let result = block_on(self.store.put_axiom(axiom)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "validateGraphJson")]
    pub fn validate_graph_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let graph_id = id_field(&value, "graphId")?;
        let ontology_id = id_field(&value, "ontologyId")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.validate_graph(&graph_id, &ontology_id, &scope))
            .map_err(to_napi_error)?;
        encode(&result)
    }
}

/// Response payload for an ingest + extract operation, returned to Node as JSON.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestExtractResponse {
    graph: KnowledgeGraph,
    entities: Vec<KnowledgeEntity>,
    relationships: Vec<KnowledgeRelationship>,
    chunk_count: usize,
}

/// Stateful local ingest + extract engine exposed to Node through N-API.
///
/// Owns one SQLite-backed knowledge store. `ingestExtractJson` runs the
/// deterministic ingest pipeline (source -> document -> chunks) and then the
/// `GraphExtractor`, persisting the graph and returning it for visualization.
#[napi]
pub struct NativeIngestEngine {
    store: SqlKnowledgeStore,
}

#[napi]
impl NativeIngestEngine {
    /// Opens a SQLite ingest engine. Pass a path for a durable file-backed store
    /// (shared with the knowledge engine when the same file is used); omit for
    /// in-memory.
    #[napi(constructor)]
    pub fn new(path: Option<String>) -> Result<Self> {
        let store = match path {
            Some(path) => SqlKnowledgeStore::open_file(path),
            None => SqlKnowledgeStore::open_in_memory(),
        }
        .map_err(to_napi_error)?;
        Ok(Self { store })
    }

    /// Ingests a document and extracts its knowledge graph in one pass.
    ///
    /// Accepts a JSON-encoded `DocumentIngestRequest`; returns a JSON-encoded
    /// graph (graph + entities + relationships + chunk count). Code documents use
    /// the `CodeSymbolChunker`; everything else uses the plain-text chunker.
    #[napi(js_name = "ingestExtractJson")]
    pub fn ingest_extract_json(&self, request_json: String) -> Result<String> {
        let request: DocumentIngestRequest = decode(&request_json)?;
        let is_code = matches!(request.document_kind, SourceDocumentKind::Code);
        let ingested = if is_code {
            block_on(KnowledgeIngestor::new(CodeSymbolChunker).ingest(&self.store, request))
                .map_err(to_napi_error)?
        } else {
            let chunker =
                PlainTextChunker::new(PlainTextChunkerOptions::default()).map_err(to_napi_error)?;
            block_on(KnowledgeIngestor::new(chunker).ingest(&self.store, request))
                .map_err(to_napi_error)?
        };
        let chunk_count = ingested.chunks.len();
        let extracted = block_on(GraphExtractor::new().extract_into(
            &self.store,
            &ingested.source,
            &ingested.document,
            &ingested.chunks,
        ))
        .map_err(to_napi_error)?;
        encode(&IngestExtractResponse {
            graph: extracted.graph,
            entities: extracted.entities,
            relationships: extracted.relationships,
            chunk_count,
        })
    }
}

fn id_field(value: &serde_json::Value, key: &str) -> Result<Id> {
    let text = value
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::from_reason(format!("missing string field '{key}'")))?;
    Ok(Id::from(text))
}

fn scope_field(value: &serde_json::Value) -> Result<Scope> {
    let scope_value = value
        .get("scope")
        .ok_or_else(|| Error::from_reason("missing 'scope' field"))?;
    serde_json::from_value::<Scope>(scope_value.clone())
        .map_err(|error| Error::from_reason(format!("invalid scope: {error}")))
}

fn decode<T>(json: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(json).map_err(|error| Error::from_reason(error.to_string()))
}

fn encode<T>(value: &T) -> Result<String>
where
    T: serde::Serialize,
{
    serde_json::to_string(value).map_err(|error| Error::from_reason(error.to_string()))
}

fn to_napi_error(error: CoreError) -> Error {
    Error::from_reason(error.to_string())
}

/// FastEmbed-powered semantic retrieval (feature-gated; the demo build enables
/// `fastembed`). Indexes chunked text with BGE-small passage embeddings into an
/// in-memory sqlite-vec index and answers queries with BGE-small query embeddings.
#[cfg(feature = "fastembed")]
mod retrieval {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use engram_domain::EmbeddingTargetType;
    use engram_ingest::{Chunker, PlainTextChunker, PlainTextChunkerOptions};
    use engram_store_vector::{FastEmbedBgeSmallQueryProvider, SqliteVectorIndex, VectorEntry};
    use napi::bindgen_prelude::*;
    use napi_derive::napi;
    use serde::{Deserialize, Serialize};

    use crate::{decode, encode, to_napi_error};

    const DIMENSIONS: u32 = 384;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct IndexRequest {
        text: String,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct IndexResponse {
        indexed: usize,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SearchRequest {
        query: String,
        top_k: Option<u32>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct SearchHit {
        id: String,
        text: String,
        score: f32,
    }

    struct State {
        index: SqliteVectorIndex,
        chunks: HashMap<String, String>,
    }

    /// Stateful local semantic-retrieval engine exposed to Node through N-API.
    #[napi]
    pub struct NativeRetrievalEngine {
        provider: FastEmbedBgeSmallQueryProvider,
        state: Mutex<State>,
    }

    #[napi]
    impl NativeRetrievalEngine {
        /// Opens the engine; constructs the FastEmbed BGE-small model (may
        /// download model assets on first use).
        #[napi(constructor)]
        pub fn new() -> Result<Self> {
            let provider = FastEmbedBgeSmallQueryProvider::new().map_err(to_napi_error)?;
            let index = SqliteVectorIndex::open_in_memory(DIMENSIONS).map_err(to_napi_error)?;
            Ok(Self {
                provider,
                state: Mutex::new(State {
                    index,
                    chunks: HashMap::new(),
                }),
            })
        }

        /// Chunks text, embeds each chunk (BGE-small passage), and indexes it.
        #[napi(js_name = "indexJson")]
        pub fn index_json(&self, request_json: String) -> Result<String> {
            let request: IndexRequest = decode(&request_json)?;
            let chunker = PlainTextChunker::new(PlainTextChunkerOptions {
                max_chars_per_chunk: 120,
            })
            .map_err(to_napi_error)?;
            let candidates = chunker.chunk(&request.text).map_err(to_napi_error)?;
            let model = self.provider.model_name().to_owned();
            // Embed before taking the state lock so reads are not blocked by inference.
            let mut entries = Vec::with_capacity(candidates.len());
            for (index, candidate) in candidates.iter().enumerate() {
                let id = format!("chunk-{index}");
                let embedding = self
                    .provider
                    .embed_passage(&candidate.text)
                    .map_err(to_napi_error)?;
                entries.push((id, candidate.text.clone(), embedding));
            }
            let mut state = self
                .state
                .lock()
                .map_err(|_| Error::from_reason("retrieval state lock poisoned"))?;
            // Indexing replaces the current index: clear vectors + chunk registry
            // so re-indexing never hits a primary-key collision and the index
            // always reflects the latest corpus.
            state.index.clear().map_err(to_napi_error)?;
            state.chunks.clear();
            let mut indexed = 0;
            for (id, text, embedding) in entries {
                state
                    .index
                    .insert(VectorEntry {
                        id: id.clone(),
                        target_type: EmbeddingTargetType::Chunk,
                        target_id: id.clone(),
                        model: model.clone(),
                        dimensions: DIMENSIONS,
                        content_hash: id.clone(),
                        embedding,
                    })
                    .map_err(to_napi_error)?;
                state.chunks.insert(id, text);
                indexed += 1;
            }
            encode(&IndexResponse { indexed })
        }

        /// Embeds the query (BGE-small query) and returns nearest chunks.
        #[napi(js_name = "searchJson")]
        pub fn search_json(&self, request_json: String) -> Result<String> {
            let request: SearchRequest = decode(&request_json)?;
            let query = self
                .provider
                .embed_query(&request.query)
                .map_err(to_napi_error)?;
            let limit = request.top_k.unwrap_or(5).max(1);
            let state = self
                .state
                .lock()
                .map_err(|_| Error::from_reason("retrieval state lock poisoned"))?;
            let hits = state.index.search(&query, limit).map_err(to_napi_error)?;
            let results: Vec<SearchHit> = hits
                .iter()
                .map(|hit| SearchHit {
                    id: hit.target_id.clone(),
                    text: state
                        .chunks
                        .get(&hit.target_id)
                        .cloned()
                        .unwrap_or_default(),
                    score: 1.0 / (1.0 + hit.distance.max(0.0)),
                })
                .collect();
            encode(&results)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::{ContextPayload, ForgetResult, WriteMemoryResponse};

    fn write_fixture() -> String {
        include_str!("../../../contracts/v1/examples/write-memory-request.json").to_owned()
    }

    fn retrieval_fixture() -> String {
        include_str!("../../../contracts/v1/examples/retrieval-request.json").to_owned()
    }

    fn forget_request(memory_id: &str) -> String {
        format!(
            r#"{{
                "targetType": "memory",
                "targetId": "{memory_id}",
                "scope": {{
                    "tenant": "tenant-demo",
                    "workspace": "engram",
                    "environment": "local"
                }},
                "requester": {{
                    "actor": {{
                        "id": "actor-agent-1",
                        "kind": "agent",
                        "displayName": "Contract Agent"
                    }},
                    "roles": ["maintainer"],
                    "permissions": ["memory.forget"]
                }},
                "mode": "delete",
                "reason": "native bridge test"
            }}"#
        )
    }

    #[test]
    fn native_engine_round_trips_write_retrieve_and_forget_json() {
        let engine = NativeMemoryEngine::new(None).expect("engine");

        let write_response = engine
            .write_memory_json(write_fixture())
            .expect("write memory");
        let write_response =
            serde_json::from_str::<WriteMemoryResponse>(&write_response).expect("write response");

        let context = engine
            .retrieve_json(retrieval_fixture())
            .expect("retrieve context");
        let context = serde_json::from_str::<ContextPayload>(&context).expect("context");
        assert_eq!(context.items.len(), 1);

        let result = engine
            .forget_json(forget_request(&write_response.record.id.to_string()))
            .expect("forget memory");
        let result = serde_json::from_str::<ForgetResult>(&result).expect("forget result");
        assert_eq!(result.status, engram_domain::ForgetStatus::Deleted);
    }
}
