//! Graph operations for the knowledge engine.
//!
//! Manages knowledge graph operations through the KnowledgeGraphRepository port.

use engram_domain::KnowledgeGraph;
use engram_knowledge::{KnowledgeGraphRepository, OntologyRepository};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use std::sync::Arc;

use crate::{decode, encode, id_field, scope_field, to_napi_error};

/// Stores or updates a knowledge graph.
pub fn put_graph_json(store: &Arc<SqlKnowledgeStore>, graph_json: String) -> Result<String> {
    let graph: KnowledgeGraph = decode(&graph_json)?;
    let result = block_on(store.put_graph(graph)).map_err(to_napi_error)?;
    encode(&result)
}

/// Retrieves a graph by ID and scope.
pub fn get_graph_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let id = id_field(&value, "id")?;
    let scope = scope_field(&value)?;
    let result = block_on(store.get_graph(&id, &scope)).map_err(to_napi_error)?;
    encode(&result)
}

/// Retrieves neighbors for a node in a graph.
pub fn neighbors_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let graph_id = id_field(&value, "graphId")?;
    let node_id = id_field(&value, "nodeId")?;
    let scope = scope_field(&value)?;
    let limit = value
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let result =
        block_on(store.neighbors(&graph_id, &node_id, &scope, limit)).map_err(to_napi_error)?;
    encode(&result)
}

/// Lists all graphs in the given scope.
pub fn list_graphs_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let scope = scope_field(&value)?;
    let result = block_on(store.list_graphs(&scope)).map_err(to_napi_error)?;
    encode(&result)
}

/// Validates a graph against an ontology.
pub fn validate_graph_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let graph_id = id_field(&value, "graphId")?;
    let ontology_id = id_field(&value, "ontologyId")?;
    let scope = scope_field(&value)?;
    let result =
        block_on(store.validate_graph(&graph_id, &ontology_id, &scope)).map_err(to_napi_error)?;
    encode(&result)
}
