//! Ingest operations for the Node-API bridge.
//!
//! Repository scanning and document ingestion operations with background job support.

use engram_domain::{Actor, Policy, Scope, SourceDocumentKind};
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, GraphExtractor, KnowledgeIngestor, PlainTextChunker,
    PlainTextChunkerOptions, ScanOptions, scan_repository,
};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use serde::Serialize;
use std::path::PathBuf;

use crate::{decode, encode, scope_field, to_napi_error};

use super::ingest_state::ScanJobState;
use super::resolve_cross_file_edges;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestExtractResponse {
    graph: engram_domain::KnowledgeGraph,
    entities: Vec<engram_domain::KnowledgeEntity>,
    relationships: Vec<engram_domain::KnowledgeRelationship>,
    chunk_count: usize,
}

/// Provides ingest operations for the Node-API bridge.
pub struct IngestOps {
    store: SqlKnowledgeStore,
    jobs: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, ScanJobState>>>,
    job_counter: std::sync::atomic::AtomicU64,
}

impl IngestOps {
    pub fn new(store: SqlKnowledgeStore) -> Self {
        Self {
            store,
            jobs: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            job_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Starts a background repository scan; returns `{ jobId }` immediately.
    /// Progress is read via `getScanJobJson`. The request carries
    /// `{ root, scope, policy, actor, sourceName, maxBytes, manifestPath }`.
    pub fn start_scan_job_json(&mut self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let root = value
            .get("root")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::from_reason("missing 'root'"))?
            .to_owned();
        let source_name = value
            .get("sourceName")
            .and_then(|v| v.as_str())
            .unwrap_or("scan")
            .to_owned();
        let max_bytes = value.get("maxBytes").and_then(|v| v.as_u64()).unwrap_or(0);
        let manifest_path = value
            .get("manifestPath")
            .and_then(|v| v.as_str())
            .map(PathBuf::from);
        let scope: Scope = scope_field(&value)?;
        let policy: Policy = serde_json::from_value(
            value
                .get("policy")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )
        .map_err(|e| Error::from_reason(e.to_string()))?;
        let actor: Actor = serde_json::from_value(
            value
                .get("actor")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )
        .map_err(|e| Error::from_reason(e.to_string()))?;

        // Load the prior manifest (incremental resume). Skip when force=true so
        // every file is re-ingested (e.g. after an extractor change).
        let force = value
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let prior = if force {
            Default::default()
        } else {
            manifest_path
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|s| {
                    serde_json::from_str::<std::collections::HashMap<String, String>>(&s).ok()
                })
                .unwrap_or_default()
        };

        let job_id = format!(
            "job-{}",
            self.job_counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        self.jobs
            .lock()
            .map_err(|_| Error::from_reason("job lock poisoned"))?
            .insert(job_id.clone(), ScanJobState::running());

        let store = self.store.clone();
        let jobs = self.jobs.clone();
        // Spawn a background Rust thread that runs the parallel scan and updates
        // the shared job state. No N-API calls cross the thread boundary — Node
        // polls via getScanJobJson.
        let job_id_for_thread = job_id.clone();
        std::thread::spawn(move || {
            let opts = ScanOptions {
                scope,
                policy,
                actor,
                source_name: source_name.clone(),
                max_bytes,
                manifest: prior,
            };
            let progress = |p: engram_ingest::ScanProgress| {
                if let Ok(mut jobs) = jobs.lock() {
                    if let Some(state) = jobs.get_mut(&job_id_for_thread) {
                        state.current_file = Some(p.file);
                        state.processed += 1;
                        match p.status {
                            "ingested" => state.ingested += 1,
                            "unchanged" => state.unchanged += 1,
                            "skipped" => state.skipped += 1,
                            "error" => state.errors += 1,
                            _ => {}
                        }
                    }
                }
            };
            let result = scan_repository(PathBuf::from(&root).as_path(), &opts, &store, progress);
            // Cross-file resolution: after the parallel scan, connect entities
            // that share a name across different graphs so the Q&A + explorer
            // see cross-file/cross-repo edges.
            if let Ok((ref summary, _)) = result {
                if summary.ingested > 0 {
                    resolve_cross_file_edges(&store, &opts.scope);
                }
            }
            let final_state = match result {
                Ok((summary, new_manifest)) => {
                    if let Some(path) = manifest_path {
                        let _ = std::fs::write(
                            &path,
                            serde_json::to_string(&new_manifest).unwrap_or_default(),
                        );
                    }
                    ScanJobState {
                        processed: summary.ingested
                            + summary.unchanged
                            + summary.skipped
                            + summary.errors,
                        ingested: summary.ingested,
                        unchanged: summary.unchanged,
                        skipped: summary.skipped,
                        errors: summary.errors,
                        status: "done".to_owned(),
                        summary: Some(summary),
                        ..ScanJobState::running()
                    }
                }
                Err(e) => ScanJobState {
                    status: "error".to_owned(),
                    error: Some(e.to_string()),
                    ..ScanJobState::running()
                },
            };
            if let Ok(mut jobs) = jobs.lock() {
                jobs.insert(job_id_for_thread, final_state);
            }
        });

        encode(&serde_json::json!({ "jobId": job_id }))
    }

    /// Reads the current state of a scan job: `{ status, currentFile, processed,
    /// ingested, unchanged, skipped, errors, summary, error }`.
    pub fn get_scan_job_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let job_id = value
            .get("jobId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::from_reason("missing 'jobId'"))?;
        let jobs = self
            .jobs
            .lock()
            .map_err(|_| Error::from_reason("job lock poisoned"))?;
        let state = jobs.get(job_id).cloned().unwrap_or(ScanJobState {
            status: "unknown".to_owned(),
            ..ScanJobState::running()
        });
        encode(&state)
    }

    /// Ingests a document and extracts its knowledge graph in one pass.
    ///
    /// Accepts a JSON-encoded `DocumentIngestRequest`; returns a JSON-encoded
    /// graph (graph + entities + relationships + chunk count). Code documents use
    /// the `CodeSymbolChunker`; everything else uses the plain-text chunker.
    pub fn ingest_extract_json(&self, request_json: String) -> Result<String> {
        let request: DocumentIngestRequest = decode(&request_json)?;
        let is_code = matches!(request.document_kind, SourceDocumentKind::Code);
        let ingested = if is_code {
            block_on(KnowledgeIngestor::new(CodeSymbolChunker).ingest(&self.store, request))
                .map_err(to_napi_error)?
        } else {
            let chunker =
                PlainTextChunker::new(PlainTextChunkerOptions::default()).map_err(to_napi_error)?;
            block_on(KnowledgeIngestor::new(chunker).ingest(&self.store, request))
                .map_err(to_napi_error)?
        };
        let chunk_count = ingested.chunks.len();
        let extracted = block_on(GraphExtractor::new().extract_into(
            &self.store,
            &ingested.source,
            &ingested.document,
            &ingested.chunks,
        ))
        .map_err(to_napi_error)?;
        encode(&IngestExtractResponse {
            graph: extracted.graph,
            entities: extracted.entities,
            relationships: extracted.relationships,
            chunk_count,
        })
    }
}
