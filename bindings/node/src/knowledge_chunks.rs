//! Chunk operations for the knowledge engine.
//!
//! Manages knowledge chunk operations through the KnowledgeRepository port.

use engram_domain::KnowledgeChunk;
use engram_knowledge::KnowledgeRepository;
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use std::sync::Arc;

use crate::{decode, encode, id_field, scope_field, to_napi_error};

/// Stores or updates a knowledge chunk.
pub fn put_chunk_json(store: &Arc<SqlKnowledgeStore>, chunk_json: String) -> Result<String> {
    let chunk: KnowledgeChunk = decode(&chunk_json)?;
    let result = block_on(store.put_chunk(chunk)).map_err(to_napi_error)?;
    encode(&result)
}

/// Retrieves a chunk by ID and scope.
pub fn get_chunk_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let id = id_field(&value, "id")?;
    let scope = scope_field(&value)?;
    let result = block_on(store.get_chunk(&id, &scope)).map_err(to_napi_error)?;
    encode(&result)
}

/// Lists all chunks in the given scope.
pub fn list_chunks_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let scope = scope_field(&value)?;
    let result = block_on(store.list_chunks(&scope)).map_err(to_napi_error)?;
    encode(&result)
}
