//! Source operations for the knowledge engine.
//!
//! Manages knowledge source (repository) operations through the KnowledgeRepository port.

use engram_domain::KnowledgeSource;
use engram_knowledge::KnowledgeRepository;
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use std::sync::Arc;

use crate::{decode, encode, scope_field, to_napi_error};

/// Stores or updates a knowledge source (repository metadata).
pub fn put_source_json(store: &Arc<SqlKnowledgeStore>, source_json: String) -> Result<String> {
    let source: KnowledgeSource = decode(&source_json)?;
    let result = block_on(store.put_source(source)).map_err(to_napi_error)?;
    encode(&result)
}

/// Lists all sources in the given scope.
pub fn list_sources_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let scope = scope_field(&value)?;
    let result = block_on(store.list_sources(&scope)).map_err(to_napi_error)?;
    encode(&result)
}
