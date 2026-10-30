//! Ingest operations for the Node-API bridge.
//!
//! Repository scanning and document ingestion operations with background job support.

use engram_domain::{Actor, CodeBinding, Policy, Scope, SourceDocumentKind};
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, GraphExtractor, KnowledgeIngestor, PlainTextChunker,
    PlainTextChunkerOptions, ScanOptions, scan_repository,
};
use engram_knowledge::CodeBindingRepository as _;
use engram_store_sqlite::SqlKnowledgeStore;
use engram_store_sqlite::knowledge::SqlCodeBindingRepository;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use serde::Serialize;
use std::path::PathBuf;

use crate::{decode, encode, scope_field, to_napi_error};

use super::ingest_state::ScanJobState;
use super::resolve_cross_file_edges;

/// Default write policy when a legacy scan request omits `policy`
/// (TASK-020: the binding gate is the authority; payload ergonomics default
/// to the same durable workspace-visible policy the provider scan uses).
fn default_scan_policy() -> Policy {
    Policy {
        visibility: engram_domain::Visibility::Workspace,
        retention: engram_domain::Retention::Durable,
        sensitivity: Some(engram_domain::Sensitivity::Low),
        allowed_uses: vec![engram_domain::AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(engram_domain::DeleteMode::Tombstone),
    }
}

/// Default system actor when a legacy scan request omits `actor`.
fn default_scan_actor() -> Actor {
    Actor {
        id: engram_domain::Id::from("engram-node-legacy"),
        kind: engram_domain::ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestExtractResponse {
    graph: engram_domain::KnowledgeGraph,
    entities: Vec<engram_domain::KnowledgeEntity>,
    relationships: Vec<engram_domain::KnowledgeRelationship>,
    chunk_count: usize,
}

/// Shared handle to the durable binding cell for engines that hold it by
/// value (TASK-020): `Arc`-wraps the cell so sibling engines (knowledge)
/// can read eligibility without owning a second connection.
pub struct BindingCell {
    inner: std::sync::Arc<SqlCodeBindingRepository>,
}

impl BindingCell {
    pub fn new(cell: SqlCodeBindingRepository) -> Self {
        Self {
            inner: std::sync::Arc::new(cell),
        }
    }
    pub fn cell(&self) -> &SqlCodeBindingRepository {
        &self.inner
    }
}

/// Provides ingest operations for the Node-API bridge.
pub struct IngestOps {
    store: SqlKnowledgeStore,
    /// The durable code-binding cell (TASK-020): code scans require a
    /// persisted binding — the legacy engine consults the same durable
    /// authority as the provider path, never a checkout-derived binding.
    bindings: SqlCodeBindingRepository,
    jobs: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, ScanJobState>>>,
    job_counter: std::sync::atomic::AtomicU64,
}

impl IngestOps {
    pub fn new(store: SqlKnowledgeStore, bindings: SqlCodeBindingRepository) -> Self {
        Self {
            store,
            bindings,
            jobs: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            job_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Binds a repository branch durably (TASK-020): same durable first-bind
    /// semantics as the neutral coordinator's cell.
    pub fn code_bind_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let repo = value
            .get("repo")
            .and_then(|field| field.as_str())
            .ok_or_else(|| Error::from_reason("missing 'repo'"))?
            .to_owned();
        let branch_ref = value
            .get("branchRef")
            .and_then(|field| field.as_str())
            .ok_or_else(|| Error::from_reason("missing 'branchRef'"))?
            .to_owned();
        let binding = CodeBinding {
            scope,
            repo_id: engram_domain::RepoId(repo),
            branch_ref,
            version: 1,
            active_generation: None,
            epoch: 0,
        };
        let bound = block_on(self.bindings.bind(binding)).map_err(to_napi_error)?;
        encode(&bound)
    }

    /// TASK-020 binding gate: the durable cell, not the caller's path, is
    /// the authority for code scans. Returns the bound ref on success.
    fn require_bound_ref(
        &self,
        scope: &Scope,
        repo: &str,
        asserted_branch: Option<&str>,
    ) -> Result<String> {
        let binding = block_on(self.bindings.get_binding(scope, &engram_domain::RepoId(repo.to_owned())))
            .map_err(to_napi_error)?
            .ok_or_else(|| {
                Error::from_reason(
                    "BindingRequired: code scans require a persisted binding; bind the repository first",
                )
            })?;
        if let Some(asserted) = asserted_branch
            && asserted != binding.branch_ref
        {
            return Err(Error::from_reason(
                "BindingConflict: asserted branch does not match the bound ref",
            ));
        }
        Ok(binding.branch_ref)
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
        let policy: Policy = match value.get("policy") {
            None => default_scan_policy(),
            Some(policy) => serde_json::from_value(policy.clone()).map_err(|error| {
                Error::from_reason(format!("InvalidPolicy: {error}"))
            })?,
        };
        let actor: Actor = match value.get("actor") {
            None => default_scan_actor(),
            Some(actor) => serde_json::from_value(actor.clone()).map_err(|error| {
                Error::from_reason(format!("InvalidActor: {error}"))
            })?,
        };

        // TASK-020: persisted binding is mandatory for code scans.
        let repo = value
            .get("repo")
            .and_then(|field| field.as_str())
            .ok_or_else(|| {
                Error::from_reason(
                    "BindingRequired: code scans require a persisted binding; bind the repository first",
                )
            })?
            .to_owned();
        let asserted_branch = value.get("branchRef").and_then(|field| field.as_str());
        self.require_bound_ref(&scope, &repo, asserted_branch)?;

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
                scan_filter: engram_ingest::ScanFilter::default(),
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
            None,
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
