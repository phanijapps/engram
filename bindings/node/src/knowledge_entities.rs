//! Entity operations for the knowledge engine.
//!
//! Manages knowledge entity operations through the KnowledgeRepository port.

use engram_domain::KnowledgeEntity;
use engram_knowledge::KnowledgeRepository;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use std::sync::Arc;

use crate::{decode, encode, id_field, scope_field, to_napi_error};

/// Stores or updates a knowledge entity.
pub fn put_entity_json(store: &Arc<SqlKnowledgeStore>, entity_json: String) -> Result<String> {
    let entity: KnowledgeEntity = decode(&entity_json)?;
    let result = block_on(store.put_entity(entity)).map_err(to_napi_error)?;
    encode(&result)
}

/// Retrieves an entity by ID and scope.
pub fn get_entity_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let id = id_field(&value, "id")?;
    let scope = scope_field(&value)?;
    let result = block_on(store.get_entity(&id, &scope)).map_err(to_napi_error)?;
    encode(&result)
}

/// Lists all entities in the given scope.
pub fn list_entities_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let scope = scope_field(&value)?;
    let result = block_on(store.list_entities(&scope)).map_err(to_napi_error)?;
    encode(&result)
}

/// Lists entities belonging to a specific source graph (by stable_source_key).
pub fn list_entities_by_source_json(
    store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let scope = scope_field(&value)?;
    let source_key = value["stableSourceKey"]
        .as_str()
        .ok_or_else(|| Error::from_reason("missing stableSourceKey"))?
        .to_owned();
    let result =
        block_on(store.list_entities_by_source(&scope, &source_key)).map_err(to_napi_error)?;
    encode(&result)
}
