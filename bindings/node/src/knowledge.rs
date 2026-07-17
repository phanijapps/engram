//! Knowledge engine for Node-API bridge.
//!
//! Stateful local knowledge + taxonomy engine exposed to Node through N-API.
//! Owns one SQLite-backed `SqlKnowledgeStore` so graph, source, and taxonomy
//! calls observe the same scoped state. The methods are JSON transports over
//! the `engram-knowledge` ports; TypeScript owns ergonomics.

use engram_store_sqlite::SqlKnowledgeStore;
use engram_store_lexical::LexicalIndex;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::{Arc, Mutex};

// Import plain functions from operation modules
use crate::codegraph::{
    blast_radius_json, bridge_symbols_json, call_communities_json, central_symbols_json,
    cyclomatic_complexity_json, dead_code_json, dependency_path_json, find_api_calls_json,
    find_endpoints_json, find_entry_points_json, match_api_topology_json, process_flow_json,
};
use crate::knowledge_chunks::{get_chunk_json, list_chunks_json, put_chunk_json};
use crate::knowledge_concepts::{
    get_concept_scheme_json, list_concepts_json, put_concept_json, put_concept_relation_json,
    put_concept_scheme_json, validate_taxonomy_proposal_json,
};
use crate::knowledge_documents::put_document_json;
use crate::knowledge_entities::{
    get_entity_json, list_entities_by_source_json, list_entities_json, put_entity_json,
};
use crate::knowledge_fusion::{
    associative_graph_candidates_json, fuse_rrf_ids_json, fuse_rrf_json, graph_candidates_json,
};
use crate::knowledge_graph::{
    get_graph_json, list_graphs_json, neighbors_json, put_graph_json, validate_graph_json,
};
use crate::knowledge_ontology::{
    get_ontology_json, put_axiom_json, put_class_json, put_ontology_json, put_property_json,
};
use crate::knowledge_relationships::{
    get_relationship_json, list_relationships_by_source_json, list_relationships_json,
    put_relationship_json,
};
use crate::knowledge_sources::{list_sources_json, put_source_json};

/// Stateful local knowledge + taxonomy engine exposed to Node through N-API.
///
/// Owns one SQLite-backed `SqlKnowledgeStore` so graph, source, and taxonomy
/// calls observe the same scoped state. The methods are JSON transports over
/// the `engram-knowledge` ports; TypeScript owns ergonomics.
#[napi]
pub struct NativeKnowledgeEngine {
    store: Arc<SqlKnowledgeStore>,
    lexical: Mutex<Option<LexicalIndex>>,
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
        .map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Self {
            store: Arc::new(store),
            lexical: Mutex::new(None),
        })
    }

    // --- Graph fusion operations ---

    /// Retrieval-composition seam (RFC-0005): graph-ranked Entity/Chunk
    /// candidates for a request, as `RetrievalResult` JSON tagged
    /// `source = "graph"`, ready to RRF-fuse with vector candidates.
    #[napi(js_name = "graphCandidatesJson")]
    pub fn graph_candidates_json(&self, request_json: String) -> Result<String> {
        graph_candidates_json(&self.store, request_json)
    }

    /// Retrieval-composition seam (RFC-0005): associative (Personalized
    /// PageRank) graph-ranked Entity candidates for a request, as
    /// `RetrievalResult` JSON tagged `source = "associative_graph"`, ready to
    /// RRF-fuse with graph/vector/lexical candidates.
    #[napi(js_name = "associativeGraphCandidatesJson")]
    pub fn associative_graph_candidates_json(&self, request_json: String) -> Result<String> {
        associative_graph_candidates_json(&self.store, request_json)
    }

    // --- Codegraph queries (RFC-0012, on top of engram) ---

    /// `{scope}` -> dead-code symbol keys (zero callers on `calls` edges).
    #[napi(js_name = "deadCodeJson")]
    pub fn dead_code_json(&self, request_json: String) -> Result<String> {
        dead_code_json(&self.store, request_json)
    }

    /// `{scope, target, depth?}` -> transitive caller keys (blast radius).
    #[napi(js_name = "blastRadiusJson")]
    pub fn blast_radius_json(&self, request_json: String) -> Result<String> {
        blast_radius_json(&self.store, request_json)
    }

    /// `{scope, from, to}` -> shortest call path or `null`.
    #[napi(js_name = "dependencyPathJson")]
    pub fn dependency_path_json(&self, request_json: String) -> Result<String> {
        dependency_path_json(&self.store, request_json)
    }

    /// `{scope, limit?}` -> `[[symbol, score], ...]` by PageRank centrality.
    #[napi(js_name = "centralSymbolsJson")]
    pub fn central_symbols_json(&self, request_json: String) -> Result<String> {
        central_symbols_json(&self.store, request_json)
    }

    /// `{scope, limit?}` -> `[[symbol, score], ...]` by betweenness (bridges).
    #[napi(js_name = "bridgeSymbolsJson")]
    pub fn bridge_symbols_json(&self, request_json: String) -> Result<String> {
        bridge_symbols_json(&self.store, request_json)
    }

    /// `{scope, maxPasses?}` -> `{symbol: label}` Louvain communities.
    #[napi(js_name = "callCommunitiesJson")]
    pub fn call_communities_json(&self, request_json: String) -> Result<String> {
        call_communities_json(&self.store, request_json)
    }

    /// `{source}` -> cyclomatic complexity (integer).
    #[napi(js_name = "cyclomaticComplexityJson")]
    pub fn cyclomatic_complexity_json(&self, request_json: String) -> Result<String> {
        cyclomatic_complexity_json(request_json)
    }

    /// `{source}` -> `[{method, path}, ...]` HTTP endpoints.
    #[napi(js_name = "findEndpointsJson")]
    pub fn find_endpoints_json(&self, request_json: String) -> Result<String> {
        find_endpoints_json(request_json)
    }

    /// `{source}` -> `["/path", ...]` HTTP call-site targets.
    #[napi(js_name = "findApiCallsJson")]
    pub fn find_api_calls_json(&self, request_json: String) -> Result<String> {
        find_api_calls_json(request_json)
    }

    /// `{source}` -> `["main", ...]` entry-point function names.
    #[napi(js_name = "findEntryPointsJson")]
    pub fn find_entry_points_json(&self, request_json: String) -> Result<String> {
        find_entry_points_json(request_json)
    }

    /// `{scope, entryPoint, maxDepth?}` -> `[symbol, ...]` execution flow.
    #[napi(js_name = "processFlowJson")]
    pub fn process_flow_json(&self, request_json: String) -> Result<String> {
        process_flow_json(&self.store, request_json)
    }

    /// `{endpoints, calls}` -> cross-service API topology edges.
    #[napi(js_name = "matchApiTopologyJson")]
    pub fn match_api_topology_json(&self, request_json: String) -> Result<String> {
        match_api_topology_json(request_json)
    }

    /// Builds a BM25 lexical index from all entities in the store. Call after
    /// ingesting/indexing, then use `searchCodeJson` to query by keyword.
    #[napi(js_name = "indexForSearchJson")]
    pub fn index_for_search_json(&self, request_json: String) -> Result<String> {
        let scope: engram_domain::Scope =
            serde_json::from_str(&request_json).map_err(|e| Error::from_reason(e.to_string()))?;
        let entities = block_on(self.store.list_entities(&scope))
            .map_err(|e| Error::from_reason(e.to_string()))?;
        let index = LexicalIndex::new().map_err(|e| Error::from_reason(e.to_string()))?;
        // Batch into ONE commit — per-document upsert commits once per doc
        // (segment finalize + reader reload), which made a full-corpus build
        // O(n²) and froze the host process for >90s over ~18k entities.
        let entries: Vec<(String, String)> = entities
            .iter()
            .map(|entity| {
                (
                    entity.id.to_string(),
                    format!("{} {:?}", entity.name, entity.kind),
                )
            })
            .collect();
        index
            .upsert_batch(&entries)
            .map_err(|e| Error::from_reason(e.to_string()))?;
        let mut guard = self
            .lexical
            .lock()
            .map_err(|_| Error::from_reason("lexical lock poisoned".to_string()))?;
        *guard = Some(index);
        Ok(format!(r#"{{"indexed":{}}}"#, entities.len()))
    }

    /// BM25 keyword search over the lexical index built by `indexForSearchJson`.
    #[napi(js_name = "searchCodeJson")]
    pub fn search_code_json(&self, request_json: String) -> Result<String> {
        let value: serde_json::Value =
            serde_json::from_str(&request_json).map_err(|e| Error::from_reason(e.to_string()))?;
        let query = value["query"].as_str().unwrap_or("");
        let limit = value["limit"].as_u64().unwrap_or(20) as usize;
        let guard = self
            .lexical
            .lock()
            .map_err(|_| Error::from_reason("lexical lock poisoned".to_string()))?;
        let index = guard.as_ref().ok_or_else(|| {
            Error::from_reason(
                "lexical index not built — call indexForSearchJson first".to_string(),
            )
        })?;
        let hits = index
            .search(query, limit)
            .map_err(|e| Error::from_reason(e.to_string()))?;
        let results: Vec<serde_json::Value> = hits
            .iter()
            .map(|(id, score)| serde_json::json!({ "id": id, "score": score }))
            .collect();
        serde_json::to_string(&results).map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Retrieval-composition seam (RFC-0005): reciprocal-rank fusion of
    /// candidate lists (graph + vector) into one ranked list. Configurable
    /// strength (`k`, per-source `weights`) with defaults when omitted.
    #[napi(js_name = "fuseRrfJson")]
    pub fn fuse_rrf_json(&self, request_json: String) -> Result<String> {
        fuse_rrf_json(&self.store, request_json)
    }

    /// Retrieval-composition seam (RFC-0005): reciprocal-rank fusion of ranked
    /// id lists (e.g. graph chunk ids + vector chunk ids) into one fused order.
    /// Lightweight alternative to `fuseRrfJson` for callers that have ranked id
    /// lists, not full `RetrievalResult`s — the demo uses this to fuse graph +
    /// vector chunk orders without marshaling Provenance/Policy per candidate.
    /// The formula mirrors `ReciprocalRankFusion` (1/(k + rank)); the canonical,
    /// tested impl lives in `engram-retrieval`.
    #[napi(js_name = "fuseRrfIdsJson")]
    pub fn fuse_rrf_ids_json(&self, request_json: String) -> Result<String> {
        fuse_rrf_ids_json(&self.store, request_json)
    }

    // --- Source operations ---

    #[napi(js_name = "putSourceJson")]
    pub fn put_source_json(&self, source_json: String) -> Result<String> {
        put_source_json(&self.store, source_json)
    }

    #[napi(js_name = "listSourcesJson")]
    pub fn list_sources_json(&self, request_json: String) -> Result<String> {
        list_sources_json(&self.store, request_json)
    }

    // --- Document operations ---

    #[napi(js_name = "putDocumentJson")]
    pub fn put_document_json(&self, document_json: String) -> Result<String> {
        put_document_json(&self.store, document_json)
    }

    // --- Chunk operations ---

    #[napi(js_name = "putChunkJson")]
    pub fn put_chunk_json(&self, chunk_json: String) -> Result<String> {
        put_chunk_json(&self.store, chunk_json)
    }

    #[napi(js_name = "getChunkJson")]
    pub fn get_chunk_json(&self, request_json: String) -> Result<String> {
        get_chunk_json(&self.store, request_json)
    }

    #[napi(js_name = "listChunksJson")]
    pub fn list_chunks_json(&self, request_json: String) -> Result<String> {
        list_chunks_json(&self.store, request_json)
    }

    // --- Entity operations ---

    #[napi(js_name = "putEntityJson")]
    pub fn put_entity_json(&self, entity_json: String) -> Result<String> {
        put_entity_json(&self.store, entity_json)
    }

    #[napi(js_name = "getEntityJson")]
    pub fn get_entity_json(&self, request_json: String) -> Result<String> {
        get_entity_json(&self.store, request_json)
    }

    #[napi(js_name = "listEntitiesJson")]
    pub fn list_entities_json(&self, request_json: String) -> Result<String> {
        list_entities_json(&self.store, request_json)
    }

    #[napi(js_name = "listEntitiesBySourceJson")]
    pub fn list_entities_by_source_json(&self, request_json: String) -> Result<String> {
        list_entities_by_source_json(&self.store, request_json)
    }

    // --- Relationship operations ---

    #[napi(js_name = "putRelationshipJson")]
    pub fn put_relationship_json(&self, relationship_json: String) -> Result<String> {
        put_relationship_json(&self.store, relationship_json)
    }

    #[napi(js_name = "getRelationshipJson")]
    pub fn get_relationship_json(&self, request_json: String) -> Result<String> {
        get_relationship_json(&self.store, request_json)
    }

    #[napi(js_name = "listRelationshipsJson")]
    pub fn list_relationships_json(&self, request_json: String) -> Result<String> {
        list_relationships_json(&self.store, request_json)
    }

    #[napi(js_name = "listRelationshipsBySourceJson")]
    pub fn list_relationships_by_source_json(&self, request_json: String) -> Result<String> {
        list_relationships_by_source_json(&self.store, request_json)
    }

    // --- Graph operations ---

    #[napi(js_name = "putGraphJson")]
    pub fn put_graph_json(&self, graph_json: String) -> Result<String> {
        put_graph_json(&self.store, graph_json)
    }

    #[napi(js_name = "getGraphJson")]
    pub fn get_graph_json(&self, request_json: String) -> Result<String> {
        get_graph_json(&self.store, request_json)
    }

    #[napi(js_name = "neighborsJson")]
    pub fn neighbors_json(&self, request_json: String) -> Result<String> {
        neighbors_json(&self.store, request_json)
    }

    #[napi(js_name = "listGraphsJson")]
    pub fn list_graphs_json(&self, request_json: String) -> Result<String> {
        list_graphs_json(&self.store, request_json)
    }

    #[napi(js_name = "validateGraphJson")]
    pub fn validate_graph_json(&self, request_json: String) -> Result<String> {
        validate_graph_json(&self.store, request_json)
    }

    // --- Concept and taxonomy operations ---

    #[napi(js_name = "putConceptSchemeJson")]
    pub fn put_concept_scheme_json(&self, scheme_json: String) -> Result<String> {
        put_concept_scheme_json(&self.store, scheme_json)
    }

    #[napi(js_name = "getConceptSchemeJson")]
    pub fn get_concept_scheme_json(&self, request_json: String) -> Result<String> {
        get_concept_scheme_json(&self.store, request_json)
    }

    #[napi(js_name = "putConceptJson")]
    pub fn put_concept_json(&self, concept_json: String) -> Result<String> {
        put_concept_json(&self.store, concept_json)
    }

    #[napi(js_name = "putConceptRelationJson")]
    pub fn put_concept_relation_json(&self, relation_json: String) -> Result<String> {
        put_concept_relation_json(&self.store, relation_json)
    }

    #[napi(js_name = "listConceptsJson")]
    pub fn list_concepts_json(&self, request_json: String) -> Result<String> {
        list_concepts_json(&self.store, request_json)
    }

    /// Validates a governed taxonomy proposal using Rust-owned taxonomy rules.
    ///
    /// The input is `{ proposal, concepts, relations }`; the result is a
    /// `TaxonomyValidationReport` JSON payload. No store mutation occurs.
    #[napi(js_name = "validateTaxonomyProposalJson")]
    pub fn validate_taxonomy_proposal_json(&self, request_json: String) -> Result<String> {
        validate_taxonomy_proposal_json(&self.store, request_json)
    }

    // --- Ontology operations ---

    #[napi(js_name = "putOntologyJson")]
    pub fn put_ontology_json(&self, ontology_json: String) -> Result<String> {
        put_ontology_json(&self.store, ontology_json)
    }

    #[napi(js_name = "getOntologyJson")]
    pub fn get_ontology_json(&self, request_json: String) -> Result<String> {
        get_ontology_json(&self.store, request_json)
    }

    #[napi(js_name = "putClassJson")]
    pub fn put_class_json(&self, class_json: String) -> Result<String> {
        put_class_json(&self.store, class_json)
    }

    #[napi(js_name = "putPropertyJson")]
    pub fn put_property_json(&self, property_json: String) -> Result<String> {
        put_property_json(&self.store, property_json)
    }

    #[napi(js_name = "putAxiomJson")]
    pub fn put_axiom_json(&self, axiom_json: String) -> Result<String> {
        put_axiom_json(&self.store, axiom_json)
    }
}
