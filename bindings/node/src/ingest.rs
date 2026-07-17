//! Ingest engine for Node-API bridge.
//!
//! Stateful local ingest engine exposed to Node through N-API.
//! Owns one SQLite-backed `SqlKnowledgeStore` and manages background scan jobs.

use engram_store_sqlite::SqlKnowledgeStore;
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::ingest_ops::IngestOps;

/// Stateful local ingest engine exposed to Node through N-API.
///
/// Owns one SQLite-backed `SqlKnowledgeStore` so scan and ingest calls
/// observe the same scoped state. The methods are JSON transports over
/// the `engram-ingest` ports; TypeScript owns ergonomics.
#[napi]
pub struct NativeIngestEngine {
    ops: IngestOps,
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
        .map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Self {
            ops: IngestOps::new(store),
        })
    }

    /// Starts a background repository scan; returns `{ jobId }` immediately.
    /// Progress is read via `getScanJobJson`. The request carries
    /// `{ root, scope, policy, actor, sourceName, maxBytes, manifestPath }`.
    #[napi(js_name = "startScanJobJson")]
    pub fn start_scan_job_json(&mut self, request_json: String) -> Result<String> {
        self.ops.start_scan_job_json(request_json)
    }

    /// Reads the current state of a scan job: `{ status, currentFile, processed,
    /// ingested, unchanged, skipped, errors, summary, error }`.
    #[napi(js_name = "getScanJobJson")]
    pub fn get_scan_job_json(&self, request_json: String) -> Result<String> {
        self.ops.get_scan_job_json(request_json)
    }

    /// Ingests a document and extracts its knowledge graph in one pass.
    ///
    /// Accepts a JSON-encoded `DocumentIngestRequest`; returns a JSON-encoded
    /// graph (graph + entities + relationships + chunk count). Code documents use
    /// the `CodeSymbolChunker`; everything else uses the plain-text chunker.
    #[napi(js_name = "ingestExtractJson")]
    pub fn ingest_extract_json(&self, request_json: String) -> Result<String> {
        self.ops.ingest_extract_json(request_json)
    }
}
