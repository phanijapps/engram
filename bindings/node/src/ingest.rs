//! Ingest engine for Node-API bridge.
//!
//! Stateful local ingest engine exposed to Node through N-API.
//! Owns one SQLite-backed `SqlKnowledgeStore` and manages background scan jobs.
//! Legacy code scans and bindings are unsupported because this engine cannot
//! receive the provider host's code-authorization policy.

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
    /// in-memory. The durable binding cell opens alongside at
    /// No code-binding cell is opened because legacy binding is unsupported.
    #[napi(constructor)]
    pub fn new(path: Option<String>) -> Result<Self> {
        let store = match &path {
            Some(path) => SqlKnowledgeStore::open_file(path),
            None => SqlKnowledgeStore::open_in_memory(),
        }
        .map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Self {
            ops: IngestOps::new(store),
        })
    }

    /// Binds a repository branch durably (TASK-020): `{ scope, repo, branchRef }`
    /// → serialized `CodeBinding`. Delegates to the SAME durable first-bind
    /// semantics as the neutral coordinator's cell (same-ref idempotent;
    /// another branch `BindingConflict`) — the legacy engine never derives a
    /// binding from a checkout.
    #[napi(js_name = "codeBindJson")]
    pub fn code_bind_json(&self, request_json: String) -> Result<String> {
        self.ops.code_bind_json(request_json)
    }

    /// Legacy engines have no host authorization seam and deny reset uniformly.
    #[napi(js_name = "codeResetPreviewJson")]
    pub fn code_reset_preview_json(&self, request_json: String) -> Result<String> {
        self.ops.code_reset_preview_json(request_json)
    }
    #[napi(js_name = "codeResetApplyJson")]
    pub fn code_reset_apply_json(&self, request_json: String) -> Result<String> {
        self.ops.code_reset_apply_json(request_json)
    }
    #[napi(js_name = "codeResetResumeJson")]
    pub fn code_reset_resume_json(&self, request_json: String) -> Result<String> {
        self.ops.code_reset_resume_json(request_json)
    }

    /// Starts a background repository scan; returns `{ jobId }` immediately.
    /// Progress is read via `getScanJobJson`. The request carries
    /// `{ root, scope, repo, branchRef?, policy, actor, sourceName, maxBytes, manifestPath }`.
    ///
    /// TASK-020: `repo` (a RepoId) is REQUIRED and a persisted binding for
    /// `(scope, repo)` must exist — an unbound scope fails immediately with a
    /// `BindingRequired` denial; a mismatched `branchRef` assertion fails with
    /// `BindingConflict`.
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
