//! Relationship operations for the knowledge engine.
//!
//! Manages knowledge relationship operations through the KnowledgeRepository port.

use engram_domain::{Id, KnowledgeRelationship, Scope};
use engram_knowledge::KnowledgeRepository;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use std::sync::Arc;

use crate::{decode, encode, id_field, scope_field, to_napi_error};

/// Stores or updates a knowledge relationship.
pub fn put_relationship_json(
    store: &Arc<SqlKnowledgeStore>,
    relationship_json: String,
) -> Result<String> {
    let relationship: KnowledgeRelationship = decode(&relationship_json)?;
    let result = block_on(store.put_relationship(relationship)).map_err(to_napi_error)?;
    encode(&result)
}

/// Retrieves a relationship by ID and scope.
pub fn get_relationship_json(
    store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let id = id_field(&value, "id")?;
    let scope = scope_field(&value)?;
    let result = block_on(store.get_relationship(&id, &scope)).map_err(to_napi_error)?;
    encode(&result)
}

/// Lists all relationships in the given scope.
pub fn list_relationships_json(
    store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let scope = scope_field(&value)?;
    let result = block_on(store.list_relationships(&scope)).map_err(to_napi_error)?;
    encode(&result)
}
