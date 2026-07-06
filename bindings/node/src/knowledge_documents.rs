//! Document operations for the knowledge engine.
//!
//! Manages source document operations through the KnowledgeRepository port.

use engram_domain::SourceDocument;
use engram_knowledge::KnowledgeRepository;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use std::sync::Arc;

use crate::{decode, encode, to_napi_error};

/// Stores or updates a source document.
pub fn put_document_json(store: &Arc<SqlKnowledgeStore>, document_json: String) -> Result<String> {
    let document: SourceDocument = decode(&document_json)?;
    let result = block_on(store.put_document(document)).map_err(to_napi_error)?;
    encode(&result)
}
