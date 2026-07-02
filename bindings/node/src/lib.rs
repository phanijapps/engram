//! Node-API bridge for Engram memory operations.
//!
//! The binding is intentionally a JSON transport over Rust behavior. TypeScript
//! packages own ergonomics; this crate owns serialization round trips into the
//! Rust memory service.

use engram_core::{
    ArchitectureEvalCase, BeliefRepository, ContradictionDetector, plan_consolidation_operations,
    required_architecture_capabilities, summarize_architecture_coverage,
    validate_hierarchy_parentage,
};
use engram_domain::{
    Actor, Belief, Concept, ConceptRelation, ConceptScheme, ConsolidationRequest, Contradiction,
    ContradictionResolution, HierarchyNode, Id, KnowledgeChunk, KnowledgeEntity, KnowledgeGraph,
    KnowledgeRelationship, KnowledgeSource, Ontology, OntologyAxiom, OntologyClass,
    OntologyProperty, Policy, Scope, SourceDocument, SourceDocumentKind, TaxonomyProposal,
};
use engram_domain::{ForgetRequest, RetrievalRequest, RetrievalResult, WriteMemoryRequest};
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, GraphExtractor, KnowledgeIngestor, PlainTextChunker,
    PlainTextChunkerOptions, ScanOptions, scan_repository,
};
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
    validate_taxonomy_proposal,
};
use engram_memory::{CoreError, MemoryService};
use engram_store_belief_sqlite::SqlBeliefStore;
use engram_store_knowledge_sqlite::{GraphCandidateSource, GraphRetrievalIndex, SqlKnowledgeStore};
use engram_store_sql::SqlMemoryService;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::{Deserialize, Serialize};

use engram_retrieval::{
    DEFAULT_RRF_K, ReciprocalFusionConfig, ReciprocalRankFusion, RetrievalFusion, RetrievalIndex,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaxonomyValidationRequest {
    proposal: TaxonomyProposal,
    concepts: Vec<Concept>,
    #[serde(default)]
    relations: Vec<ConceptRelation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConsolidationPlanRequest {
    request: ConsolidationRequest,
    #[serde(default)]
    planned_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// After a scan, connects entities that share a name across different graphs.
/// For each entity whose name appears in multiple graphs, creates a "defined_in"
/// relationship so the Q&A + explorer see cross-file edges. Best-effort — errors
/// are silently ignored (the scan summary is already captured).
fn resolve_cross_file_edges(store: &SqlKnowledgeStore, scope: &Scope) {
    let entities = match block_on(store.list_entities(scope)) {
        Ok(e) => e,
        Err(_) => return,
    };
    let relationships = match block_on(store.list_relationships(scope)) {
        Ok(r) => r,
        Err(_) => return,
    };
    // Group entity IDs by name (lowercased) → Vec<(entity_id, graph_id)>.
    let mut by_name: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();
    for e in &entities {
        let name = e.name.to_lowercase();
        let gid = e
            .graph_id
            .as_ref()
            .map(|g| g.to_string())
            .unwrap_or_default();
        by_name
            .entry(name)
            .or_default()
            .push((e.id.to_string(), gid));
    }
    // Collect existing relationship keys to avoid duplicates.
    let mut existing: std::collections::HashSet<(String, String, String)> =
        std::collections::HashSet::new();
    for r in &relationships {
        existing.insert((
            r.subject
                .id
                .as_ref()
                .map(|i| i.to_string())
                .unwrap_or_default(),
            r.predicate.clone(),
            r.object
                .id
                .as_ref()
                .map(|i| i.to_string())
                .unwrap_or_default(),
        ));
    }
    // For each name that appears in multiple graphs, create cross-graph "defined_in" edges.
    let now = chrono::Utc::now();
    let prov = engram_domain::Provenance {
        source: "cross-file-resolver".to_owned(),
        actor: engram_domain::Actor {
            id: engram_domain::Id::from("engram-cross-file"),
            kind: engram_domain::ActorKind::System,
            display_name: Some("Cross-file resolver".to_owned()),
            metadata: None,
        },
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(0.8),
        method: Some("name_match_resolution".to_owned()),
    };
    let scope_owned = scope.clone();
    let _policy = engram_domain::Policy {
        visibility: engram_domain::Visibility::Workspace,
        retention: engram_domain::Retention::Durable,
        sensitivity: Some(engram_domain::Sensitivity::Low),
        allowed_uses: vec![engram_domain::AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(engram_domain::DeleteMode::Tombstone),
    };
    for entries in by_name.values() {
        if entries.len() < 2 {
            continue;
        }
        // Create bidirectional "defined_in" edges between all pairs.
        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                let (id_a, graph_a) = &entries[i];
                let (id_b, graph_b) = &entries[j];
                if graph_a == graph_b {
                    continue;
                }
                let key_ab = (id_a.clone(), "defined_in".to_owned(), id_b.clone());
                if !existing.contains(&key_ab) {
                    let rel = engram_domain::KnowledgeRelationship {
                        id: engram_domain::Id::from(format!("rel-xfile-{id_a}-{id_b}")),
                        graph_id: None,
                        subject: engram_domain::EntityRef {
                            id: Some(engram_domain::Id::from(id_a.clone())),
                            kind: None,
                            name: None,
                            aliases: Vec::new(),
                        },
                        predicate: "defined_in".to_owned(),
                        object: engram_domain::EntityRef {
                            id: Some(engram_domain::Id::from(id_b.clone())),
                            kind: None,
                            name: None,
                            aliases: Vec::new(),
                        },
                        scope: scope_owned.clone(),
                        evidence: Vec::new(),
                        confidence: Some(0.8),
                        provenance: prov.clone(),
                        created_at: now,
                        updated_at: None,
                    };
                    let _ = block_on(store.put_relationship(rel));
                }
            }
        }
    }
}

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
    store: std::sync::Arc<SqlKnowledgeStore>,
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
        Ok(Self {
            store: std::sync::Arc::new(store),
        })
    }

    /// Retrieval-composition seam (RFC-0005): graph-ranked Entity/Chunk
    /// candidates for a request, as `RetrievalResult` JSON tagged
    /// `source = "graph"`, ready to RRF-fuse with vector candidates.
    #[napi(js_name = "graphCandidatesJson")]
    pub fn graph_candidates_json(&self, request_json: String) -> Result<String> {
        let request = decode::<RetrievalRequest>(&request_json)?;
        let source: std::sync::Arc<dyn GraphCandidateSource> = self.store.clone();
        let index = GraphRetrievalIndex::new(source);
        let results = block_on(index.retrieve_candidates(&request)).map_err(to_napi_error)?;
        encode(&results)
    }

    /// Retrieval-composition seam (RFC-0005): reciprocal-rank fusion of
    /// candidate lists (graph + vector) into one ranked list. Configurable
    /// strength (`k`, per-source `weights`) with defaults when omitted.
    #[napi(js_name = "fuseRrfJson")]
    pub fn fuse_rrf_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let request: RetrievalRequest = serde_json::from_value(value["request"].clone())
            .map_err(|e| Error::from_reason(e.to_string()))?;
        let candidates: Vec<RetrievalResult> = serde_json::from_value(value["candidates"].clone())
            .map_err(|e| Error::from_reason(e.to_string()))?;
        let k = value["k"]
            .as_u64()
            .map(|n| n as u32)
            .unwrap_or(DEFAULT_RRF_K);
        let default_weight = value["defaultWeight"]
            .as_f64()
            .map(|f| f as f32)
            .unwrap_or(1.0);
        let weights: std::collections::BTreeMap<String, f32> = value
            .get("weights")
            .and_then(|w| w.as_object())
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f as f32)))
                    .collect()
            })
            .unwrap_or_default();
        let config =
            ReciprocalFusionConfig::new(k, default_weight, weights).map_err(to_napi_error)?;
        let fused = ReciprocalRankFusion::new(config)
            .fuse(&request, candidates)
            .map_err(to_napi_error)?;
        encode(&fused)
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
        let value = decode::<serde_json::Value>(&request_json)?;
        let lists: Vec<Vec<String>> = serde_json::from_value(value["lists"].clone())
            .map_err(|e| Error::from_reason(e.to_string()))?;
        let k = value["k"]
            .as_u64()
            .map(|n| n as u32)
            .unwrap_or(DEFAULT_RRF_K) as f32;
        let limit = value["limit"].as_u64().map(|n| n as usize);
        // score(id) = Σ over lists of 1/(k + rank_in_list); first-seen order for
        // a stable tiebreak. An id in two lists is boosted (cross-source consensus).
        let mut scores: std::collections::BTreeMap<String, f32> = std::collections::BTreeMap::new();
        let mut order: Vec<String> = Vec::new();
        for list in &lists {
            for (rank, id) in list.iter().enumerate() {
                let contribution = 1.0 / (k + (rank + 1) as f32);
                if scores.insert(id.clone(), 0.0).is_none() {
                    order.push(id.clone());
                }
                if let Some(s) = scores.get_mut(id) {
                    *s += contribution;
                }
            }
        }
        order.sort_by(|a, b| {
            scores[b]
                .partial_cmp(&scores[a])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if let Some(limit) = limit {
            order.truncate(limit);
        }
        encode(&order)
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

    /// Validates a governed taxonomy proposal using Rust-owned taxonomy rules.
    ///
    /// The input is `{ proposal, concepts, relations }`; the result is a
    /// `TaxonomyValidationReport` JSON payload. No store mutation occurs.
    #[napi(js_name = "validateTaxonomyProposalJson")]
    pub fn validate_taxonomy_proposal_json(&self, request_json: String) -> Result<String> {
        let request = decode::<TaxonomyValidationRequest>(&request_json)?;
        let report =
            validate_taxonomy_proposal(&request.proposal, &request.concepts, &request.relations);
        encode(&report)
    }

    // --- Whole-graph exploration (store-specific list methods) ----------------

    #[napi(js_name = "listGraphsJson")]
    pub fn list_graphs_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.list_graphs(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "listEntitiesJson")]
    pub fn list_entities_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.list_entities(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "listRelationshipsJson")]
    pub fn list_relationships_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.list_relationships(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "listChunksJson")]
    pub fn list_chunks_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.list_chunks(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "listSourcesJson")]
    pub fn list_sources_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.list_sources(&scope)).map_err(to_napi_error)?;
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

/// Stateless hierarchy behavior exposed to Node through N-API.
///
/// This transport validates hierarchy build outputs with Rust-owned rules. It
/// does not persist nodes or replace the SQLite hierarchy adapter.
#[napi]
pub struct NativeHierarchyEngine;

#[napi]
impl NativeHierarchyEngine {
    /// Creates a stateless hierarchy validation engine.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self
    }

    /// Validates hierarchy parentage for a JSON array of `HierarchyNode`.
    #[napi(js_name = "validateParentageJson")]
    pub fn validate_parentage_json(&self, nodes_json: String) -> Result<String> {
        let nodes = decode::<Vec<HierarchyNode>>(&nodes_json)?;
        validate_hierarchy_parentage(&nodes).map_err(to_napi_error)?;
        encode(&serde_json::json!({ "valid": true }))
    }
}

/// Stateless consolidation behavior exposed to Node through N-API.
///
/// Planning stays in Rust so TypeScript can display or submit consolidation
/// plans without duplicating strategy-to-operation rules.
#[napi]
pub struct NativeConsolidationEngine;

#[napi]
impl NativeConsolidationEngine {
    /// Creates a stateless consolidation planning engine.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self
    }

    /// Plans consolidation operations for `{ request, plannedAt? }`.
    #[napi(js_name = "planJson")]
    pub fn plan_json(&self, request_json: String) -> Result<String> {
        let request = decode::<ConsolidationPlanRequest>(&request_json)?;
        let planned_at = request.planned_at.unwrap_or_else(chrono::Utc::now);
        let plan =
            plan_consolidation_operations(&request.request, planned_at).map_err(to_napi_error)?;
        encode(&plan)
    }
}

/// Stateless evaluation coverage behavior exposed to Node through N-API.
///
/// The engine summarizes executed case reports already produced by Rust-backed
/// fixtures; it does not reimplement recall, leakage, or ranking checks in JS.
#[napi]
pub struct NativeEvalEngine;

#[napi]
impl NativeEvalEngine {
    /// Creates a stateless evaluation coverage engine.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self
    }

    /// Summarizes architecture coverage for a JSON array of `ArchitectureEvalCase`.
    #[napi(js_name = "architectureCoverageJson")]
    pub fn architecture_coverage_json(&self, cases_json: String) -> Result<String> {
        let cases = decode::<Vec<ArchitectureEvalCase>>(&cases_json)?;
        let coverage =
            summarize_architecture_coverage(cases, &required_architecture_capabilities());
        encode(&coverage)
    }
}

/// Stateful belief + contradiction engine exposed to Node through N-API.
///
/// Owns one SQLite-backed belief store (`SqlBeliefStore`). Implements the
/// `BeliefRepository` (put/get/resolve) + `ContradictionDetector` surface. The
/// store is distinct from knowledge + memory storage; beliefs are derived
/// stance, not source-grounded evidence. Focused per ADR-0007 (no god-struct).
#[napi]
pub struct NativeBeliefEngine {
    store: SqlBeliefStore,
}

#[napi]
impl NativeBeliefEngine {
    /// Opens a SQLite belief engine. Pass a path for a durable file-backed store;
    /// omit for in-memory.
    #[napi(constructor)]
    pub fn new(path: Option<String>) -> Result<Self> {
        let store = match path {
            Some(path) => SqlBeliefStore::open_file(path),
            None => SqlBeliefStore::open_in_memory(),
        }
        .map_err(to_napi_error)?;
        Ok(Self { store })
    }

    #[napi(js_name = "putBeliefJson")]
    pub fn put_belief_json(&self, belief_json: String) -> Result<String> {
        let belief: Belief = decode(&belief_json)?;
        let result = block_on(self.store.put_belief(belief)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "listBeliefsJson")]
    pub fn list_beliefs_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.list_beliefs(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "putContradictionJson")]
    pub fn put_contradiction_json(&self, contradiction_json: String) -> Result<String> {
        let contradiction: Contradiction = decode(&contradiction_json)?;
        let result =
            block_on(self.store.put_contradiction(contradiction)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "listContradictionsJson")]
    pub fn list_contradictions_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.list_contradictions(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "getContradictionJson")]
    pub fn get_contradiction_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.get_contradiction(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    #[napi(js_name = "resolveContradictionJson")]
    pub fn resolve_contradiction_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let resolution: ContradictionResolution = serde_json::from_value(
            value
                .get("resolution")
                .cloned()
                .ok_or_else(|| Error::from_reason("missing 'resolution' field"))?,
        )
        .map_err(|error| Error::from_reason(error.to_string()))?;
        let result = block_on(self.store.resolve_contradiction(&id, &scope, resolution))
            .map_err(to_napi_error)?;
        encode(&result)
    }

    /// Runs advisory contradiction detection over the supplied beliefs.
    #[napi(js_name = "detectContradictionsJson")]
    pub fn detect_contradictions_json(&self, beliefs_json: String) -> Result<String> {
        let beliefs: Vec<Belief> = decode(&beliefs_json)?;
        let result = block_on(self.store.detect_contradictions(&beliefs)).map_err(to_napi_error)?;
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
    jobs: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, ScanJobState>>>,
    job_counter: std::sync::atomic::AtomicU64,
}

/// Snapshot of a background scan job, returned to Node as JSON.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanJobState {
    status: String, // "running" | "done" | "error"
    current_file: Option<String>,
    processed: usize,
    ingested: usize,
    unchanged: usize,
    skipped: usize,
    errors: usize,
    summary: Option<engram_ingest::ScanSummary>,
    error: Option<String>,
}

impl ScanJobState {
    fn running() -> Self {
        Self {
            status: "running".to_owned(),
            current_file: None,
            processed: 0,
            ingested: 0,
            unchanged: 0,
            skipped: 0,
            errors: 0,
            summary: None,
            error: None,
        }
    }
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
        Ok(Self {
            store,
            jobs: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            job_counter: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Starts a background repository scan; returns `{ jobId }` immediately.
    /// Progress is read via `getScanJobJson`. The request carries
    /// `{ root, scope, policy, actor, sourceName, maxBytes, manifestPath }`.
    #[napi(js_name = "startScanJobJson")]
    pub fn start_scan_job_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let root = value
            .get("root")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::from_reason("missing 'root'"))?
            .to_owned();
        let source_name = value
            .get("sourceName")
            .and_then(|v| v.as_str())
            .unwrap_or("scan")
            .to_owned();
        let max_bytes = value.get("maxBytes").and_then(|v| v.as_u64()).unwrap_or(0);
        let manifest_path = value
            .get("manifestPath")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from);
        let scope: Scope = scope_field(&value)?;
        let policy: Policy = serde_json::from_value(
            value
                .get("policy")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )
        .map_err(|e| Error::from_reason(e.to_string()))?;
        let actor: Actor = serde_json::from_value(
            value
                .get("actor")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )
        .map_err(|e| Error::from_reason(e.to_string()))?;

        // Load the prior manifest (incremental resume). Skip when force=true so
        // every file is re-ingested (e.g. after an extractor change).
        let force = value
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let prior = if force {
            Default::default()
        } else {
            manifest_path
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|s| {
                    serde_json::from_str::<std::collections::HashMap<String, String>>(&s).ok()
                })
                .unwrap_or_default()
        };

        let job_id = format!(
            "job-{}",
            self.job_counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        self.jobs
            .lock()
            .map_err(|_| Error::from_reason("job lock poisoned"))?
            .insert(job_id.clone(), ScanJobState::running());

        let store = self.store.clone();
        let jobs = self.jobs.clone();
        // Spawn a background Rust thread that runs the parallel scan and updates
        // the shared job state. No N-API calls cross the thread boundary — Node
        // polls via getScanJobJson.
        let job_id_for_thread = job_id.clone();
        std::thread::spawn(move || {
            let opts = ScanOptions {
                scope,
                policy,
                actor,
                source_name: source_name.clone(),
                max_bytes,
                manifest: prior,
            };
            let progress = |p: engram_ingest::ScanProgress| {
                if let Ok(mut jobs) = jobs.lock() {
                    if let Some(state) = jobs.get_mut(&job_id_for_thread) {
                        state.current_file = Some(p.file);
                        state.processed += 1;
                        match p.status {
                            "ingested" => state.ingested += 1,
                            "unchanged" => state.unchanged += 1,
                            "skipped" => state.skipped += 1,
                            "error" => state.errors += 1,
                            _ => {}
                        }
                    }
                }
            };
            let result = scan_repository(std::path::Path::new(&root), &opts, &store, progress);
            // Cross-file resolution: after the parallel scan, connect entities
            // that share a name across different graphs so the Q&A + explorer
            // see cross-file/cross-repo edges.
            if let Ok((ref summary, _)) = result {
                if summary.ingested > 0 {
                    resolve_cross_file_edges(&store, &opts.scope);
                }
            }
            let final_state = match result {
                Ok((summary, new_manifest)) => {
                    if let Some(path) = manifest_path {
                        let _ = std::fs::write(
                            &path,
                            serde_json::to_string(&new_manifest).unwrap_or_default(),
                        );
                    }
                    ScanJobState {
                        processed: summary.ingested
                            + summary.unchanged
                            + summary.skipped
                            + summary.errors,
                        ingested: summary.ingested,
                        unchanged: summary.unchanged,
                        skipped: summary.skipped,
                        errors: summary.errors,
                        status: "done".to_owned(),
                        summary: Some(summary),
                        ..ScanJobState::running()
                    }
                }
                Err(e) => ScanJobState {
                    status: "error".to_owned(),
                    error: Some(e.to_string()),
                    ..ScanJobState::running()
                },
            };
            if let Ok(mut jobs) = jobs.lock() {
                jobs.insert(job_id_for_thread, final_state);
            }
        });

        encode(&serde_json::json!({ "jobId": job_id }))
    }

    /// Reads the current state of a scan job: `{ status, currentFile, processed,
    /// ingested, unchanged, skipped, errors, summary, error }`.
    #[napi(js_name = "getScanJobJson")]
    pub fn get_scan_job_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let job_id = value
            .get("jobId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::from_reason("missing 'jobId'"))?;
        let jobs = self
            .jobs
            .lock()
            .map_err(|_| Error::from_reason("job lock poisoned"))?;
        let state = jobs.get(job_id).cloned().unwrap_or(ScanJobState {
            status: "unknown".to_owned(),
            ..ScanJobState::running()
        });
        encode(&state)
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

    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct SearchRequest {
        query: String,
        top_k: Option<u32>,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct SearchHit {
        id: String,
        text: String,
        score: f32,
    }

    /// Lazy (query-time) embedding primitive: embed one named chunk idempotently.
    /// Repeated calls for the same `chunkId` skip inference entirely (cache hit).
    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct IndexChunkRequest {
        chunk_id: String,
        text: String,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct IndexChunkResponse {
        embedded: bool,
        total: usize,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct CacheStatsResponse {
        embedded: usize,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ClearResponse {
        cleared: bool,
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
        /// download model assets on first use). Pass a path for a durable
        /// file-backed vector store (embeddings persist across restarts); omit
        /// for in-memory.
        #[napi(constructor)]
        pub fn new(path: Option<String>) -> Result<Self> {
            let provider = FastEmbedBgeSmallQueryProvider::new().map_err(to_napi_error)?;
            let index = match path {
                Some(path) => SqliteVectorIndex::open(&path, DIMENSIONS),
                None => SqliteVectorIndex::open_in_memory(DIMENSIONS),
            }
            .map_err(to_napi_error)?;
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

        /// Idempotently embeds one chunk (BGE-small passage) by its stable id.
        ///
        /// The lazy-embedding primitive: a cache hit (chunkId already present)
        /// returns without running inference; a miss embeds the passage, inserts
        /// it into the sqlite-vec index keyed by `chunkId`, and records the text.
        /// Double-checks under the lock so concurrent callers can't double-embed.
        /// Embedding happens outside the state lock so cache hits (reads) aren't
        /// blocked by a miss's inference.
        #[napi(js_name = "indexChunkJson")]
        pub fn index_chunk_json(&self, request_json: String) -> Result<String> {
            let IndexChunkRequest { chunk_id, text } = decode(&request_json)?;
            // Fast path: cache hit — no inference, no write.
            {
                let state = self
                    .state
                    .lock()
                    .map_err(|_| Error::from_reason("retrieval state lock poisoned"))?;
                if state.chunks.contains_key(&chunk_id) {
                    return encode(&IndexChunkResponse {
                        embedded: false,
                        total: state.chunks.len(),
                    });
                }
            }
            // Cache miss: embed without holding the lock so reads stay responsive.
            let embedding = self.provider.embed_passage(&text).map_err(to_napi_error)?;
            let model = self.provider.model_name().to_owned();
            let mut state = self
                .state
                .lock()
                .map_err(|_| Error::from_reason("retrieval state lock poisoned"))?;
            // Double-check: another caller may have embedded this id meanwhile.
            if state.chunks.contains_key(&chunk_id) {
                return encode(&IndexChunkResponse {
                    embedded: false,
                    total: state.chunks.len(),
                });
            }
            state
                .index
                .insert(VectorEntry {
                    id: chunk_id.clone(),
                    target_type: EmbeddingTargetType::Chunk,
                    target_id: chunk_id.clone(),
                    model,
                    dimensions: DIMENSIONS,
                    content_hash: chunk_id.clone(),
                    embedding,
                })
                .map_err(to_napi_error)?;
            state.chunks.insert(chunk_id, text);
            encode(&IndexChunkResponse {
                embedded: true,
                total: state.chunks.len(),
            })
        }

        /// Reports how many chunks are currently embedded (cache coverage numerator).
        #[napi(js_name = "cacheStatsJson")]
        pub fn cache_stats_json(&self) -> Result<String> {
            let state = self
                .state
                .lock()
                .map_err(|_| Error::from_reason("retrieval state lock poisoned"))?;
            encode(&CacheStatsResponse {
                embedded: state.chunks.len(),
            })
        }

        /// Clears the embedded-chunk cache + vector index (cold start for the
        /// warm-up benchmark).
        #[napi(js_name = "clearJson")]
        pub fn clear_json(&self) -> Result<String> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| Error::from_reason("retrieval state lock poisoned"))?;
            state.index.clear().map_err(to_napi_error)?;
            state.chunks.clear();
            encode(&ClearResponse { cleared: true })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // These exercise the full engine, which loads the BGE-small model on
        // construction. They are `#[ignore]` so `cargo test` stays hermetic;
        // run them explicitly when the FastEmbed model cache is populated:
        //   cargo test -p engram-node --features fastembed --lib retrieval:: --ignored
        #[test]
        #[ignore]
        fn index_chunk_is_idempotent() {
            let engine = NativeRetrievalEngine::new().expect("engine");
            let req = encode(&IndexChunkRequest {
                chunk_id: "chunk-1".to_owned(),
                text: "The renderer paints text to the screen.".to_owned(),
            })
            .expect("encode req");
            let first: IndexChunkResponse =
                serde_json::from_str(&engine.index_chunk_json(req.clone()).expect("first"))
                    .expect("decode first");
            let second: IndexChunkResponse =
                serde_json::from_str(&engine.index_chunk_json(req).expect("second"))
                    .expect("decode second");
            assert!(first.embedded, "first call should embed");
            assert!(!second.embedded, "second call should be a cache hit");
            assert_eq!(first.total, second.total, "total must not grow on hit");
        }

        #[test]
        #[ignore]
        fn cache_stats_and_clear_track_coverage() {
            let engine = NativeRetrievalEngine::new().expect("engine");
            for i in 0..3 {
                let req = encode(&IndexChunkRequest {
                    chunk_id: format!("c{i}"),
                    text: format!("passage number {i} about rendering text"),
                })
                .expect("encode req");
                let resp: IndexChunkResponse =
                    serde_json::from_str(&engine.index_chunk_json(req).expect("index"))
                        .expect("decode");
                assert_eq!(resp.total, (i + 1) as usize);
            }
            let stats: CacheStatsResponse =
                serde_json::from_str(&engine.cache_stats_json().expect("stats"))
                    .expect("decode stats");
            assert_eq!(stats.embedded, 3);

            let _: ClearResponse =
                serde_json::from_str(&engine.clear_json().expect("clear")).expect("decode clear");
            let after: CacheStatsResponse =
                serde_json::from_str(&engine.cache_stats_json().expect("stats"))
                    .expect("decode stats");
            assert_eq!(after.embedded, 0, "clear must empty the cache");
        }

        #[test]
        #[ignore]
        fn search_returns_stored_chunk_ids() {
            let engine = NativeRetrievalEngine::new().expect("engine");
            let req = encode(&IndexChunkRequest {
                chunk_id: "real-chunk-42".to_owned(),
                text: "TerminalHandle wraps the connection handle.".to_owned(),
            })
            .expect("encode req");
            let _: IndexChunkResponse =
                serde_json::from_str(&engine.index_chunk_json(req).expect("index"))
                    .expect("decode");
            let sreq = encode(&SearchRequest {
                query: "what is the terminal handle".to_owned(),
                top_k: Some(3),
            })
            .expect("encode search");
            let hits: Vec<SearchHit> =
                serde_json::from_str(&engine.search_json(sreq).expect("search")).expect("decode");
            assert!(hits.iter().any(|h| h.id == "real-chunk-42"));
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
