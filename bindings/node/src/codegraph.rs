//! Codegraph query bindings.
//!
//! Exposes `engram-codegraph-queries` over the SQLite knowledge store as JSON,
//! mirroring the graph-fusion helpers in `knowledge_fusion.rs`. Each helper
//! reads a `scope` from the request JSON, enumerates that scope's
//! `KnowledgeRelationship`s, runs the query, and encodes the result. The
//! `NativeKnowledgeEngine` methods in `knowledge.rs` delegate here.

use std::sync::Arc;

use engram_codegraph_queries as cgq;
use engram_domain::{KnowledgeRelationship, Scope};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use serde_json::Value;

use crate::{encode, to_napi_error};

fn json_error(error: serde_json::Error) -> Error {
    Error::from_reason(error.to_string())
}

fn scope_of(value: &Value) -> Result<Scope> {
    serde_json::from_value(value["scope"].clone()).map_err(json_error)
}

fn relationships_for(
    store: &Arc<SqlKnowledgeStore>,
    scope: &Scope,
) -> Result<Vec<KnowledgeRelationship>> {
    block_on(store.list_relationships(scope)).map_err(to_napi_error)
}

/// `{scope}` -> dead-code symbol keys (sorted). Mirrors memtrace `find_dead_code`.
pub fn dead_code_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let relationships = relationships_for(store, &scope)?;
    encode(&cgq::dead_code(&relationships))
}

/// `{scope, target, depth?}` -> transitive caller keys (blast radius).
pub fn blast_radius_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let relationships = relationships_for(store, &scope)?;
    let target = value["target"].as_str().unwrap_or("").to_owned();
    let depth = value["depth"].as_u64().unwrap_or(5) as usize;
    let mut callers: Vec<String> = cgq::blast_radius(&relationships, &target, depth)
        .into_iter()
        .collect();
    callers.sort();
    encode(&callers)
}

/// `{scope, from, to}` -> shortest call path (`[symbol, ...]`) or `null`.
pub fn dependency_path_json(
    store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let relationships = relationships_for(store, &scope)?;
    let from = value["from"].as_str().unwrap_or("").to_owned();
    let to = value["to"].as_str().unwrap_or("").to_owned();
    encode(&cgq::dependency_path(&relationships, &from, &to))
}

/// `{scope, limit?}` -> `[[symbol, score], ...]` ranked by PageRank centrality.
pub fn central_symbols_json(
    store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let relationships = relationships_for(store, &scope)?;
    let limit = value["limit"].as_u64().unwrap_or(20) as usize;
    encode(&cgq::central_symbols(&relationships, limit))
}

/// `{scope, limit?}` -> `[[symbol, score], ...]` ranked by betweenness (bridges).
pub fn bridge_symbols_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let relationships = relationships_for(store, &scope)?;
    let limit = value["limit"].as_u64().unwrap_or(20) as usize;
    encode(&cgq::bridge_symbols(&relationships, limit))
}

/// `{scope, maxPasses?}` -> `{symbol: label}` Louvain communities.
pub fn call_communities_json(
    store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let relationships = relationships_for(store, &scope)?;
    let max_passes = value["maxPasses"].as_u64().unwrap_or(10) as usize;
    encode(&cgq::call_communities(&relationships, max_passes))
}
