//! Parallel repository scanner (RFC 0004 background-repo-indexer).
//!
//! Walks a root `.gitignore`-aware, applies the Slice-1 data-custody controls
//! (path confinement, secret-file blocklist, deny list, size bound), then ingests
//! the readable files in parallel with rayon. The pure filter helpers are unit-
//! tested; the security controls are ported from `demo/backend/src/decide.ts`
//! and must not be relaxed.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult, KnowledgeGraphRepository, KnowledgeRepository};
use futures::executor::block_on;
use rayon::prelude::*;

use crate::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
    PlainTextChunker, PlainTextChunkerOptions,
    classifier::{classify_file, is_denylisted, is_secret_file, is_within_root},
    content_hash, contract,
    git_detect::detect_git,
    reconcile, stable_source_key,
};

pub use crate::classifier::FileKind;

const DEFAULT_MAX_BYTES: u64 = 1024 * 1024; // 1 MiB per file
const WORKSPACE_MARKER: &str = ".engram-workspace";

/// Summary returned by a scan.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ScanSummary {
    pub scanned: usize,
    pub ingested: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub entities: usize,
    pub relationships: usize,
    pub errors: usize,
    pub git_remote: Option<String>,
    pub git_branch: Option<String>,
    pub git_sha: Option<String>,
}

/// Per-file progress emitted during a scan.
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub file: String,
    pub status: &'static str, // "ingested" | "unchanged" | "skipped" | "error"
}

/// Options for a scan.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub scope: Scope,
    pub policy: Policy,
    pub actor: Actor,
    pub source_name: String,
    pub max_bytes: u64,
    /// Prior manifest (rel path -> content hash) for incremental skip-unchanged.
    pub manifest: std::collections::HashMap<String, String>,
}

impl ScanOptions {
    pub fn max_bytes(&self) -> u64 {
        if self.max_bytes == 0 {
            DEFAULT_MAX_BYTES
        } else {
            self.max_bytes
        }
    }
}

#[derive(Debug)]
enum Outcome {
    Ingested {
        entities: usize,
        relationships: usize,
        hash: (String, String),
        /// Normalized contract keys emitted for this file when it was detected
        /// as a valid OpenAPI document. Empty for non-OpenAPI files.
        contract_keys: Vec<String>,
        /// True when the file contained an `openapi:`/`swagger:` version marker
        /// but failed to parse (malformed/truncated document). The caller
        /// increments `ScanSummary.skipped` and logs a warning.
        contract_parse_failed: bool,
        /// True when at least one entity or edge write failed during contract
        /// extraction. Keys for failed ops are not recorded in the manifest.
        /// The caller increments `ScanSummary.skipped` and logs a warning.
        contract_had_write_error: bool,
    },
    /// Produced only by the TOCTOU defense-in-depth guard inside the parallel
    /// phase.  `Unchanged` detection now happens in the serial pre-pass and is
    /// never emitted by the parallel closure.
    Skipped,
    Error,
}

/// Walks `root` and ingests readable files in parallel into `repo`.
///
/// Security: every path is canonicalized and confined under `root` (rejecting
/// `..`/symlink escape); secret-laden files are skipped by name without being
/// read; per-file size is bounded; `.gitignore` is honored via the `ignore`
/// crate. Returns the summary + the new manifest (rel path -> hash) for the
/// caller to persist.
pub fn scan_repository<R>(
    root: &Path,
    opts: &ScanOptions,
    repo: &R,
    progress: impl Fn(ScanProgress) + Send + Sync,
) -> CoreResult<(ScanSummary, std::collections::HashMap<String, String>)>
where
    R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync,
{
    let root_canonical = std::fs::canonicalize(root).map_err(|e| CoreError::InvalidRequest {
        reason: format!("cannot canonicalize scan root {}: {e}", root.display()),
    })?;

    let mut summary = ScanSummary::default();

    // Detect git metadata (remote, branch, SHA) if the root is a git repo.
    let git = detect_git(&root_canonical);
    if let Some((ref remote, ref branch, ref sha)) = git {
        summary.git_remote = Some(remote.clone());
        summary.git_branch = Some(branch.clone());
        summary.git_sha = Some(sha.clone());
    }

    // Enrich the source name with git info so it flows to every entity's
    // provenance + the Q&A citations.
    let source_name = match &git {
        Some((remote, branch, sha)) => {
            format!("{} [{}@{}:{}]", opts.source_name, remote, branch, sha)
        }
        None => opts.source_name.clone(),
    };

    // Tag git-backed sources as GitRepository and derive a SHA-free stable key
    // (does not change across commits) so each per-document KnowledgeGraph can
    // be attributed to its repository without embedding the commit SHA.
    let doc_source_kind = if git.is_some() {
        SourceKind::GitRepository
    } else {
        SourceKind::Filesystem
    };
    let source_key = {
        let remote = git.as_ref().map(|(r, _, _)| r.as_str());
        stable_source_key(remote, &opts.source_name)
    };

    let code_ingestor = KnowledgeIngestor::new(CodeSymbolChunker);
    let text_ingestor =
        KnowledgeIngestor::new(PlainTextChunker::new(PlainTextChunkerOptions::default())?);
    let ts_chunker = crate::tree_sitter_chunker::TreeSitterChunker::new().ok();
    let extractor = GraphExtractor::new();

    // Walk + filter (sequential).
    let walker = ignore::WalkBuilder::new(&root_canonical)
        .follow_links(false)
        .build();
    let mut readable: Vec<(PathBuf, FileKind, String)> = Vec::new();
    // FIX 3: Track every rel path observed during the walk — including files
    // that pass canonicalization but are subsequently skipped by the denylist,
    // classifier, or size-bound filters.  Removed-path detection uses this set
    // so that a transiently-filtered or newly-oversize file present on disk is
    // NOT mistaken for a genuinely-absent removal (adversarial Concern 4).
    let mut observed_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                summary.errors += 1;
                continue;
            }
        };
        summary.scanned += 1;
        let ft = match entry.file_type() {
            Some(ft) => ft,
            None => continue,
        };
        if !ft.is_file() {
            continue;
        }
        let path = entry.path();
        // Path confinement: canonicalize + assert within root (catches symlink escape).
        let canonical = match std::fs::canonicalize(path) {
            Ok(c) => c,
            Err(_) => {
                summary.skipped += 1;
                continue;
            }
        };
        if !is_within_root(&canonical, &root_canonical) {
            summary.skipped += 1;
            continue;
        }
        let rel = canonical
            .strip_prefix(&root_canonical)
            .map_err(|e| CoreError::InvalidRequest {
                reason: format!("rel path: {e}"),
            })?
            .to_string_lossy()
            .to_string();
        // Observe the path BEFORE any filter that could skip it so that
        // skipped-but-present files are never classified as removals (FIX 3).
        observed_paths.insert(rel.clone());
        if is_denylisted(&rel) || is_secret_file(&rel) {
            summary.skipped += 1;
            continue;
        }
        let Some(kind) = classify_file(&rel) else {
            summary.skipped += 1;
            continue;
        };
        let size = match std::fs::metadata(path) {
            Ok(m) => m.len(),
            Err(_) => {
                summary.skipped += 1;
                continue;
            }
        };
        if size > opts.max_bytes() {
            summary.skipped += 1;
            continue;
        }
        readable.push((canonical, kind, rel));
    }

    // FIX 1(a) + FIX 2: Serial pre-pass — read each file, detect changed vs
    // unchanged, and for changed/new files delete their prior graph(s) BEFORE
    // the parallel write pass (serializes the reconcile to eliminate the
    // list→delete→recount→delete-repo-node race).
    //
    // On delete failure: increment `summary.errors` and do NOT forward the
    // path to ingest — keeping the prior graph intact prevents duplicates
    // (AC-4).  The old hash is preserved in the manifest below so the path is
    // retried next scan (adversarial Concern 2 / FIX 2).
    let max_bytes = opts.max_bytes();

    // File content read in this pass is carried through to the parallel phase
    // so bytes are not read twice.
    struct ReadyToIngest {
        kind: FileKind,
        rel: String,
        content: Vec<u8>,
        hash: String,
    }

    let mut unchanged_rels: Vec<String> = Vec::new();
    let mut delete_failed: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut to_ingest: Vec<ReadyToIngest> = Vec::new();

    for (canonical, kind, rel) in &readable {
        let bytes = match std::fs::read(canonical) {
            Ok(b) => b,
            Err(_) => {
                summary.errors += 1;
                continue;
            }
        };
        if bytes.len() as u64 > max_bytes {
            // File grew between the stat-based walk check and the read
            // (TOCTOU edge).  Treat as skipped; prior graph persists.
            summary.skipped += 1;
            continue;
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let hash = content_hash(&text);
        if opts.manifest.get(rel.as_str()).is_some_and(|h| h == &hash) {
            unchanged_rels.push(rel.clone());
            continue;
        }
        // File is new or changed — delete prior graph(s) before writing.
        match block_on(reconcile::delete_prior_graphs_for_path(
            repo,
            &opts.scope,
            &source_key,
            rel,
        )) {
            Ok(()) => to_ingest.push(ReadyToIngest {
                kind: *kind,
                rel: rel.clone(),
                content: bytes,
                hash,
            }),
            Err(_) => {
                // Delete failed: surface the error, skip the write (no
                // duplicate graph), retain old hash in manifest for retry.
                summary.errors += 1;
                delete_failed.insert(rel.clone());
            }
        }
    }

    // FIX 1(b): Parallel ingest — no reconcile / delete calls inside this
    // closure.  Content was already read and prior graphs were already deleted
    // in the serial pre-pass above.

    // Pre-pass: collect ALL entity names globally so AST call extraction can
    // match cross-file callees. Without this, extract_calls only knows about
    // symbols declared in the current file, dropping most cross-file edges.
    let global_entity_names: Arc<HashSet<String>> = Arc::new(
        to_ingest
            .par_iter()
            .filter_map(|item| {
                let ext = item.rel.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
                let ts = ts_chunker.as_ref()?;
                if !ts.supports(ext) {
                    return None;
                }
                let text = String::from_utf8_lossy(&item.content);
                let candidates = ts.chunk_with_ext(&text, ext).ok()?;
                Some(
                    candidates
                        .iter()
                        .filter_map(|c| {
                            c.location
                                .as_ref()
                                .and_then(|l| l.anchor.as_deref())
                                .and_then(|a| a.split_once(' ').map(|x| x.1).map(|s| s.to_owned()))
                        })
                        .collect::<Vec<String>>(),
                )
            })
            .flatten()
            .collect(),
    );

    let name_index: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let outcomes: Vec<(String, Outcome)> = to_ingest
        .par_iter()
        .map(|item| {
            let rel = &item.rel;
            let kind = item.kind;
            if item.content.len() as u64 > max_bytes {
                // Defense-in-depth TOCTOU guard (shouldn't fire; pre-pass
                // already checked).
                return (rel.clone(), Outcome::Skipped);
            }
            let text = String::from_utf8_lossy(&item.content).into_owned();
            let text_for_ast = text.clone(); // keep a copy for AST call extraction
            let hash = item.hash.clone();
            let document_kind = match kind {
                FileKind::Code => SourceDocumentKind::Code,
                FileKind::Text => SourceDocumentKind::Text,
            };
            let request = DocumentIngestRequest {
                source_kind: doc_source_kind.clone(),
                source_name: source_name.clone(),
                scope: opts.scope.clone(),
                document_kind,
                document: DocumentMetadata {
                    path: Some(rel.clone()),
                    ..Default::default()
                },
                text: String::new(), // placeholder — real text used for chunking below
                policy: opts.policy.clone(),
                actor: opts.actor.clone(),
                stable_source_key: Some(source_key.clone()),
            };
            // Tree-sitter chunking for supported extensions; fallback to the
            // ingestor's internal chunker for others.
            let ext = rel.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
            let ingested = if let Some(ref ts) = ts_chunker {
                if ts.supports(ext) {
                    let candidates = match ts.chunk_with_ext(&text, ext) {
                        Ok(c) => c,
                        Err(_) => return (rel.clone(), Outcome::Error),
                    };
                    let mut req = request;
                    req.text = text;
                    block_on(code_ingestor.ingest_with_candidates(repo, req, candidates))
                } else {
                    let mut req = request;
                    req.text = text;
                    match kind {
                        FileKind::Code => block_on(code_ingestor.ingest(repo, req)),
                        FileKind::Text => block_on(text_ingestor.ingest(repo, req)),
                    }
                }
            } else {
                let mut req = request;
                req.text = text;
                match kind {
                    FileKind::Code => block_on(code_ingestor.ingest(repo, req)),
                    FileKind::Text => block_on(text_ingestor.ingest(repo, req)),
                }
            };
            let ingested = match ingested {
                Ok(i) => i,
                Err(_) => return (rel.clone(), Outcome::Error),
            };
            // AST-level call extraction using the GLOBAL entity name set (not
            // just this file's symbols). This preserves cross-file call edges.
            let ast_calls = if let Some(ref ts) = ts_chunker {
                if ts.supports(ext) && !global_entity_names.is_empty() {
                    ts.extract_calls(&text_for_ast, ext, &global_entity_names)
                        .ok()
                } else {
                    None
                }
            } else {
                None
            };
            let extracted_result = if let Some(ref calls) = ast_calls {
                extractor
                    .extract_with_calls(
                        &ingested.source,
                        &ingested.document,
                        &ingested.chunks,
                        Some(calls),
                    )
                    .map(|graph| {
                        // Persist manually (extract_with_calls doesn't persist).
                        let graph2 = graph.clone();
                        (graph, graph2)
                    })
                    .map(|(graph, _)| graph)
            } else {
                core::result::Result::Err(CoreError::InvalidRequest {
                    reason: "no ast calls".to_owned(),
                })
            };
            let extracted = match extracted_result {
                Ok(mut g) => {
                    // C1: cross-file resolution — register entities + resolve refs.
                    if let Ok(mut idx) = name_index.lock() {
                        for entity in &g.entities {
                            idx.insert(entity.name.clone(), entity.id.to_string());
                        }
                        for rel in &mut g.relationships {
                            if rel.predicate == "calls" && rel.object.id.is_none() {
                                if let Some(name) = &rel.object.name {
                                    if let Some(id) = idx.get(name) {
                                        rel.object.id = Some(Id::from(id.clone()));
                                    }
                                }
                            }
                        }
                    }
                    // Persist the graph + entities + relationships.
                    let _ = block_on(async {
                        repo.put_graph(g.graph.clone()).await?;
                        for entity in &g.entities {
                            repo.put_entity(entity.clone()).await?;
                        }
                        for rel in &g.relationships {
                            repo.put_relationship(rel.clone()).await?;
                        }
                        for (chunk_idx, entity_refs) in &g.chunk_entities {
                            if let Some(chunk) = ingested.chunks.get(*chunk_idx) {
                                let mut updated = chunk.clone();
                                updated.entities = entity_refs.clone();
                                repo.put_chunk(updated).await?;
                            }
                        }
                        Ok::<(), CoreError>(())
                    });
                    g
                }
                Err(_) => {
                    let mut idx = name_index.lock().unwrap();
                    match block_on(extractor.extract_into(
                        repo,
                        &ingested.source,
                        &ingested.document,
                        &ingested.chunks,
                        Some(&mut *idx),
                    )) {
                        Ok(e) => e,
                        Err(_) => return (rel.clone(), Outcome::Error),
                    }
                }
            };
            // Contract extraction (T4/T5/T6): for YAML/JSON files, attempt
            // OpenAPI detection and emit EntityKind::Api entities + exposes edges.
            // Uses `text_for_ast` (a pre-move clone of the file text) so we do
            // not need to read the content a second time.
            let (contract_keys, contract_parse_failed, contract_had_write_error) =
                extract_contract_entities(
                    repo,
                    &opts.scope,
                    &source_key,
                    &text_for_ast,
                    ext,
                    &ingested.source.provenance,
                );

            (
                rel.clone(),
                Outcome::Ingested {
                    entities: extracted.entities.len(),
                    relationships: extracted.relationships.len(),
                    hash: (rel.clone(), hash),
                    contract_keys,
                    contract_parse_failed,
                    contract_had_write_error,
                },
            )
        })
        .collect();

    // FIX 1(c): Serial post-pass — delete graphs for paths that were in the
    // prior manifest but were never observed during this scan (genuinely-absent
    // files).  Uses `observed_paths` (FIX 3) rather than `readable` so that
    // oversize / denylisted / unclassifiable files that are still present on
    // disk are not treated as removals.
    // Exclude `contract:*` manifest entries — they are metadata keys, not file
    // paths, so they must never be treated as "removed files" and must not
    // trigger a `delete_prior_graphs_for_path` call.
    let removed_paths: Vec<String> = opts
        .manifest
        .keys()
        .filter(|k| !k.starts_with("contract:") && !observed_paths.contains(*k))
        .cloned()
        .collect();
    let mut removed_delete_failed: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for removed_path in &removed_paths {
        match block_on(reconcile::delete_prior_graphs_for_path(
            repo,
            &opts.scope,
            &source_key,
            removed_path,
        )) {
            Ok(()) => {}
            Err(_) => {
                // Surface the error and keep the path in the manifest so it
                // is retried next scan rather than silently orphaned (FIX 2).
                summary.errors += 1;
                removed_delete_failed.insert(removed_path.clone());
            }
        }
    }
    // GC: check whether the per-source Repository node can be pruned now that
    // all removed-path deletions are complete.  Run once here — never before a
    // replacement write — because a replacement write re-puts the repo node via
    // upsert so pre-write GC would only widen the transient-absence window
    // (FIX 1 / adversarial Nit 6).  Errors are ignored; orphaned repo nodes
    // survive until the next scan.
    let _ = block_on(reconcile::maybe_delete_repo_node(
        repo,
        &opts.scope,
        &source_key,
    ));

    // Build the emitted manifest from this scan's outcomes.
    let mut new_manifest = std::collections::HashMap::new();

    // Accumulate contract keys emitted during the parallel phase so we can do
    // T8 retraction and update the contract manifest in the serial post-pass.
    let mut current_contract_ops: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    // Files whose contract entity/edge write failed this scan: their hash is not
    // recorded (reprocess next scan) and their prior declarations carry forward,
    // so a transient write failure cannot permanently retract a still-declared op.
    let mut write_error_rels: Vec<String> = Vec::new();

    for (rel, outcome) in outcomes {
        match outcome {
            Outcome::Ingested {
                entities,
                relationships,
                hash: (r, h),
                contract_keys,
                contract_parse_failed,
                contract_had_write_error,
            } => {
                summary.ingested += 1;
                summary.entities += entities;
                summary.relationships += relationships;
                // On a contract write error, do NOT record the file hash — the
                // file is reprocessed next scan so the still-declared op re-emits
                // (its prior keys are also folded into current_union below, so no
                // transient retraction occurs either).
                if contract_had_write_error {
                    write_error_rels.push(rel.clone());
                } else {
                    new_manifest.insert(r, h);
                }
                // T6: malformed OpenAPI — increment skipped + warn.
                if contract_parse_failed {
                    summary.skipped += 1;
                    eprintln!(
                        "[engram-ingest] warning: malformed/truncated OpenAPI document skipped during contract extraction: {rel}"
                    );
                }
                // Surface write errors: at least one entity/edge persist failed.
                if contract_had_write_error {
                    summary.skipped += 1;
                    eprintln!(
                        "[engram-ingest] warning: one or more contract entity/edge writes failed for: {rel}"
                    );
                }
                // Collect contract keys for T8 retraction + manifest. On a write
                // error, skip this file's (partial) keys — its prior declarations
                // carry forward via write_error_rels instead of being emitted.
                if !contract_had_write_error && !contract_keys.is_empty() {
                    current_contract_ops.insert(rel.clone(), contract_keys);
                }
                progress(ScanProgress {
                    file: rel,
                    status: "ingested",
                });
            }
            Outcome::Skipped => {
                // TOCTOU defense-in-depth path; increments skipped, not errors.
                summary.skipped += 1;
                progress(ScanProgress {
                    file: rel,
                    status: "skipped",
                });
            }
            Outcome::Error => {
                summary.errors += 1;
                progress(ScanProgress {
                    file: rel,
                    status: "error",
                });
            }
        }
    }

    // T8: Per-source full-declared-set retraction.
    //
    // Contract entity and edge identities are keyed per-SOURCE (not per-file),
    // so retraction must compare the source's FULL declared-key set across scans.
    //
    // prior_union  = union of all manifest["contract:<rel>"] entries from the
    //                previous scan (every file that declared ops last time).
    //
    // current_union = freshly-parsed files' keys (current_contract_ops)
    //               ∪ unchanged files' keys from manifest (still declared)
    //               ∪ delete_failed files' keys from manifest (not re-processed;
    //                 kept for retry — their prior declarations still stand)
    //               ∪ removed_delete_failed files' keys from manifest (removal
    //                 failed; kept in manifest for retry — treat as still present)
    //
    // Keys in (prior_union − current_union) represent operations that no file in
    // this source still declares. They are retracted exactly once per source,
    // covering both intra-source file changes and genuine file removals.
    let prior_union: HashSet<String> = opts
        .manifest
        .iter()
        .filter(|(k, _)| k.starts_with("contract:"))
        .flat_map(|(_, v)| serde_json::from_str::<Vec<String>>(v).unwrap_or_default())
        .collect();

    // Start with freshly-parsed keys.
    let mut current_union: HashSet<String> = current_contract_ops
        .values()
        .flat_map(|keys| keys.iter().cloned())
        .collect();
    // Keys from unchanged files (still present and declared).
    for rel in &unchanged_rels {
        let mk = format!("contract:{rel}");
        if let Some(json) = opts.manifest.get(&mk) {
            current_union.extend(serde_json::from_str::<Vec<String>>(json).unwrap_or_default());
        }
    }
    // Keys from files whose graph-delete failed (not re-processed this scan;
    // declarations carry forward until the retry succeeds).
    for rel in &delete_failed {
        let mk = format!("contract:{rel}");
        if let Some(json) = opts.manifest.get(&mk) {
            current_union.extend(serde_json::from_str::<Vec<String>>(json).unwrap_or_default());
        }
    }
    // Keys from removed paths where delete failed (kept in manifest for retry).
    for rel in &removed_delete_failed {
        let mk = format!("contract:{rel}");
        if let Some(json) = opts.manifest.get(&mk) {
            current_union.extend(serde_json::from_str::<Vec<String>>(json).unwrap_or_default());
        }
    }
    // Keys from files whose contract write failed this scan: not re-emitted, but
    // their prior declarations still stand (the file reprocesses next scan), so a
    // transient write failure cannot retract a still-declared op.
    for rel in &write_error_rels {
        let mk = format!("contract:{rel}");
        if let Some(json) = opts.manifest.get(&mk) {
            current_union.extend(serde_json::from_str::<Vec<String>>(json).unwrap_or_default());
        }
    }

    for removed_key in prior_union.difference(&current_union) {
        match block_on(contract::retract_contract_op(
            repo,
            &opts.scope,
            &source_key,
            removed_key,
        )) {
            Ok(()) => {}
            Err(e) => {
                eprintln!(
                    "[engram-ingest] warning: failed to retract contract op '{removed_key}': {e}"
                );
                summary.skipped += 1;
            }
        }
    }

    // Carry forward hashes for files whose content did not change this scan.
    for rel in unchanged_rels {
        summary.unchanged += 1;
        if let Some(h) = opts.manifest.get(&rel) {
            new_manifest.insert(rel.clone(), h.clone());
        }
        // Carry forward contract manifest entry for unchanged files.
        let ck = format!("contract:{rel}");
        if let Some(j) = opts.manifest.get(&ck) {
            new_manifest.insert(ck, j.clone());
        }
        progress(ScanProgress {
            file: rel,
            status: "unchanged",
        });
    }

    // Emit contract manifest entries for files processed in this scan.
    for (rel, keys) in &current_contract_ops {
        if !keys.is_empty() {
            if let Ok(json) = serde_json::to_string(keys) {
                new_manifest.insert(format!("contract:{rel}"), json);
            }
        }
    }

    // Carry forward the prior contract manifest entry for write-error files (their
    // file hash was not recorded, so they reprocess next scan; keep the prior
    // declaration meanwhile so prior_union stays consistent).
    for rel in &write_error_rels {
        let ck = format!("contract:{rel}");
        if let Some(j) = opts.manifest.get(&ck) {
            new_manifest.insert(ck, j.clone());
        }
    }

    // FIX 2: Files where the pre-pass delete failed — keep the old hash so
    // the path is retried next scan.  The prior graph was NOT deleted, so no
    // duplicate graph exists and the state is consistent.
    for rel in &delete_failed {
        if let Some(h) = opts.manifest.get(rel.as_str()) {
            new_manifest.insert(rel.clone(), h.clone());
        }
        // Also carry forward contract manifest entries for these files.
        let ck = format!("contract:{rel}");
        if let Some(j) = opts.manifest.get(&ck) {
            new_manifest.insert(ck, j.clone());
        }
    }

    // FIX 2: Removed paths where the post-pass delete failed — keep in the
    // manifest so they are retried rather than being silently pruned.
    for rel in &removed_delete_failed {
        if let Some(h) = opts.manifest.get(rel.as_str()) {
            new_manifest.insert(rel.clone(), h.clone());
        }
        // Also carry forward contract manifest entries for these files.
        let ck = format!("contract:{rel}");
        if let Some(j) = opts.manifest.get(&ck) {
            new_manifest.insert(ck, j.clone());
        }
    }

    Ok((summary, new_manifest))
}

/// Detects workspace children: if `root` contains a `.engram-workspace` marker,
/// returns child directories that are git repos. Returns `None` if no marker or
/// no child repos. (B8 — workspace fusion, RFC-0008)
pub fn detect_workspace(root: &Path) -> Option<Vec<PathBuf>> {
    if !root.join(WORKSPACE_MARKER).exists() {
        return None;
    }
    let children: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
        .map(|e| e.path())
        .filter(|p| p.join(".git").exists())
        .collect();
    if children.is_empty() {
        None
    } else {
        Some(children)
    }
}

/// Scans a workspace: detects child repos via `.engram-workspace` marker and
/// scans each into the shared repository with the shared workspace scope. (B8)
pub fn scan_workspace<R>(
    root: &Path,
    opts: &ScanOptions,
    repo: &R,
    progress: &(impl Fn(ScanProgress) + Send + Sync),
) -> CoreResult<ScanSummary>
where
    R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync,
{
    let children = detect_workspace(root).ok_or_else(|| CoreError::InvalidRequest {
        reason: format!("no {WORKSPACE_MARKER} marker at {}", root.display()),
    })?;
    let mut summary = ScanSummary::default();
    for child in &children {
        let repo_name = child.file_name().and_then(|n| n.to_str()).unwrap_or("repo");
        let child_opts = ScanOptions {
            source_name: format!("{}:{repo_name}", opts.source_name),
            ..opts.clone()
        };
        let (child_summary, _) = scan_repository(child, &child_opts, repo, progress)?;
        summary.scanned += child_summary.scanned;
        summary.ingested += child_summary.ingested;
        summary.unchanged += child_summary.unchanged;
        summary.skipped += child_summary.skipped;
        summary.entities += child_summary.entities;
        summary.relationships += child_summary.relationships;
        summary.errors += child_summary.errors;
    }
    Ok(summary)
}

/// Attempts OpenAPI contract extraction for a single file during the parallel
/// ingest phase (T4 entity emission, T5 source-ref union, T6 skip-and-warn).
///
/// Returns `(contract_keys, parse_failed, had_write_error)`:
/// - `contract_keys`: normalized keys for operations whose entity AND edge were
///   successfully persisted; empty for non-OpenAPI files or total-parse-failure.
///   Keys for individual write failures are excluded so the manifest does not
///   record unpersisted ops.
/// - `parse_failed`: `true` when the file had an OpenAPI marker but could not
///   be parsed; the caller increments `ScanSummary.skipped`.
/// - `had_write_error`: `true` when at least one entity or edge persist failed;
///   the caller increments `ScanSummary.skipped` and logs a warning.
fn extract_contract_entities<R>(
    repo: &R,
    scope: &Scope,
    stable_source_key: &str,
    text: &str,
    ext: &str,
    provenance: &Provenance,
) -> (Vec<String>, bool, bool)
where
    R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync,
{
    use chrono::Utc;

    match contract::detect_and_parse_openapi(text, ext) {
        Ok(None) => (Vec::new(), false, false),
        Err(_) => (Vec::new(), true, false), // malformed OpenAPI: caller increments skipped
        Ok(Some(ops)) => {
            let now = Utc::now();
            let mut keys = Vec::with_capacity(ops.len());
            let mut had_write_error = false;
            for op in &ops {
                let entity =
                    contract::build_api_entity(scope, stable_source_key, op, provenance, now);
                // Read-modify-write union for cross-repo merge (T5).
                match block_on(contract::upsert_api_entity_with_source_ref(
                    repo, scope, entity,
                )) {
                    Ok(()) => {
                        let rel = contract::build_exposes_rel(
                            scope,
                            stable_source_key,
                            op,
                            provenance,
                            now,
                        );
                        match block_on(repo.put_relationship(rel)) {
                            Ok(_) => {
                                // Both entity and edge persisted — record the key.
                                keys.push(op.normalized_key.clone());
                            }
                            Err(e) => {
                                eprintln!(
                                    "[engram-ingest] warning: failed to persist exposes edge \
                                     for '{}': {e}",
                                    op.normalized_key
                                );
                                had_write_error = true;
                                // Do NOT push key — manifest must not record an
                                // unpersisted op.
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "[engram-ingest] warning: failed to persist Api entity for '{}': {e}",
                            op.normalized_key
                        );
                        had_write_error = true;
                    }
                }
            }
            (keys, false, had_write_error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_root_checks_prefix() {
        let root = Path::new("/tmp/repo");
        assert!(is_within_root(Path::new("/tmp/repo"), root));
        assert!(is_within_root(Path::new("/tmp/repo/src/a.rs"), root));
        assert!(!is_within_root(Path::new("/tmp/other"), root));
        assert!(!is_within_root(Path::new("/tmp/repo-evil"), root));
    }

    #[test]
    fn detect_workspace_finds_child_git_repos() {
        let tmp = std::env::temp_dir().join(format!("engram-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join(".engram-workspace"), "").unwrap();
        std::fs::create_dir_all(tmp.join("frontend").join(".git")).unwrap();
        std::fs::create_dir_all(tmp.join("backend").join(".git")).unwrap();
        std::fs::create_dir_all(tmp.join("not-a-repo")).unwrap();

        let children = detect_workspace(&tmp).expect("workspace detected");
        assert_eq!(children.len(), 2);
        assert!(children.iter().any(|c| c.ends_with("frontend")));
        assert!(children.iter().any(|c| c.ends_with("backend")));

        // No marker → None.
        let no_marker = tmp.join("not-a-repo");
        assert!(detect_workspace(&no_marker).is_none());

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
