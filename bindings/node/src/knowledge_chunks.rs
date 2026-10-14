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
    encode_chunk_projection(&result, value.get("readerVersion").and_then(|v| v.as_u64()))
}

/// Lists all chunks in the given scope.
pub fn list_chunks_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let scope = scope_field(&value)?;
    let result = block_on(store.list_chunks(&scope)).map_err(to_napi_error)?;
    encode_chunk_projection(&result, value.get("readerVersion").and_then(|v| v.as_u64()))
}

/// Lists the chunks of one document within the given scope (RFC-0020 T4). Used
/// by the extract-knowledge op to read a document's chunks per-document (bounded
/// per-document, unlike the scope-wide `list_chunks`).
pub fn list_chunks_by_document_json(
    store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let document_id = id_field(&value, "documentId")?;
    let scope = scope_field(&value)?;
    let result =
        block_on(store.list_chunks_by_document(&document_id, &scope)).map_err(to_napi_error)?;
    encode_chunk_projection(&result, value.get("readerVersion").and_then(|v| v.as_u64()))
}

/// Explicit pre-source-contract projection for strict legacy JSON consumers.
/// Reader 1 is the current optional-field projection; reader 0 removes only
/// source additions. Snapshot eligibility is enforced before either projection.
pub(crate) fn encode_chunk_projection<T: serde::Serialize>(
    records: &T,
    reader: Option<u64>,
) -> Result<String> {
    let mut value = serde_json::to_value(records).map_err(|e| Error::from_reason(e.to_string()))?;
    match reader.unwrap_or(1) {
        0 => {
            fn strip(value: &mut serde_json::Value) {
                if let Some(object) = value.as_object_mut() {
                    object.remove("code");
                    object.remove("documentKind");
                }
            }
            if let Some(array) = value.as_array_mut() {
                for item in array {
                    strip(item);
                }
            } else {
                strip(&mut value);
            }
        }
        1 => {}
        _ => {
            return Err(to_napi_error(
                engram_runtime::CoreError::CapabilityUnsupported {
                    capability: "code_chunk_reader".into(),
                    reason: "reader version unsupported".into(),
                },
            ));
        }
    }
    encode(&value)
}
