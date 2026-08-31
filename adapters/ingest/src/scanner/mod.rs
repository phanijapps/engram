//! Parallel repository scanner (RFC 0004 background-repo-indexer).
//!
//! Walks a root `.gitignore`-aware, applies the Slice-1 data-custody controls
//! (path confinement, secret-file blocklist, deny list, size bound), then ingests
//! the readable files in parallel with rayon. The pure filter helpers are unit-
//! tested; the security controls are ported from `demo/backend/src/decide.ts`
//! and must not be relaxed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult, KnowledgeGraphRepository, KnowledgeRepository};
use futures::executor::block_on;
use rayon::prelude::*;

use crate::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
    MarkdownChunker, PlainTextChunker, PlainTextChunkerOptions,
    classifier::{
        classify_file, is_secret_file, is_within_root, looks_minified_bytes, looks_minified_name,
    },
    content_hash, contract,
    git_detect::detect_git,
    reconcile,
    scan_filter::ScanFilter,
    stable_source_key,
};

mod contract_phase;
mod workspace;

pub use workspace::{detect_workspace, scan_workspace};

/// `true` for Markdown extensions routed through the structure-aware chunker.
fn is_markdown_ext(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown")
}

pub use crate::classifier::FileKind;

const DEFAULT_MAX_BYTES: u64 = 1024 * 1024; // 1 MiB per file
/// Prefix for contract-op manifest keys. Uses the ASCII Unit Separator (U+001F)
/// which cannot appear in a file path on any OS, so a repo file named
/// `contract:...` cannot collide with contract-op manifest entries.
const CONTRACT_PREFIX: &str = "\u{1f}contract:";

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
    /// Tunable concept-link + file-denylist filter. Defaults to the builtin
    /// (prior hardcoded) behavior; a host merges a user config into it.
    pub scan_filter: ScanFilter,
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

    // RFC-0020 rev: clean git provenance keys (repository = remote URL, branch,
    // revision = SHA) stamped on each document's KnowledgeSource.metadata so they
    // flow to entity `record_json` (reachable via the cc entity-detail route).
    // Branch is provenance-only — re-indexing from a different branch updates it;
    // one logical entity per function regardless of branch. Built once here and
    // carried into every per-document request.
    let git_source_metadata = git.as_ref().map(|(remote, branch, sha)| {
        let mut m = engram_domain::Metadata::default();
        m.insert(
            crate::source_key::REPOSITORY_KEY.to_owned(),
            serde_json::Value::String(remote.clone()),
        );
        m.insert(
            crate::source_key::BRANCH_KEY.to_owned(),
            serde_json::Value::String(branch.clone()),
        );
        m.insert(
            crate::source_key::REVISION_KEY.to_owned(),
            serde_json::Value::String(sha.clone()),
        );
        m
    });

    let code_ingestor = KnowledgeIngestor::new(CodeSymbolChunker);
    let text_ingestor =
        KnowledgeIngestor::new(PlainTextChunker::new(PlainTextChunkerOptions::default())?);
    let markdown_ingestor = KnowledgeIngestor::new(MarkdownChunker::new()?);
    let ts_chunker = crate::TreeSitterChunker::new().ok();
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
    // code-graph-quality [minified-vendor-noise]: paths filtered THIS scan by
    // the minified-asset rules (name or content). A path here that was in the
    // prior manifest was previously indexed and is retracted below — filter
    // upgrades clean existing stores instead of leaving stale bundle symbols.
    let mut filtered_paths: HashSet<String> = HashSet::new();
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
                // Retain a prior-manifest path hit by a transient canonicalize/I/O
                // error so it is NOT mistaken for a removal (otherwise its graph is
                // deleted and only re-ingested next scan). The path is still skipped
                // for this scan; new/transient files remain skip-and-forget.
                if let Ok(rel) = path.strip_prefix(&root_canonical) {
                    let rel = rel.to_string_lossy().to_string();
                    if !rel.is_empty() && opts.manifest.contains_key(&rel) {
                        observed_paths.insert(rel);
                    }
                }
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
        if opts.scan_filter.is_denylisted(&rel) || is_secret_file(&rel) {
            summary.skipped += 1;
            continue;
        }
        // code-graph-quality [minified-vendor-noise]: minified/bundled asset
        // names are a stable, non-transient filter signal. Skipped at walk
        // time; if such a file was previously INGESTED (present in the prior
        // manifest) the pre-pass retracts its graph — a filter upgrade must
        // clean existing stores, not just future ones.
        if looks_minified_name(&rel) {
            summary.skipped += 1;
            filtered_paths.insert(rel.clone());
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
    // Trim-empty files (0-byte or whitespace-only): nothing to index, but the
    // path still goes through prior-graph retraction + manifest recording so
    // a file that BECAME empty retracts cleanly and later scans treat it as
    // unchanged. Previously these reached the chunker, which rejects empty
    // text by contract — surfacing as scan `errors` (empty-file-fix).
    let mut empty_rels: Vec<(String, String)> = Vec::new();

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
        // code-graph-quality [minified-vendor-noise]: content heuristic —
        // bundled assets with innocuous names (e.g. `app.js`) are caught by
        // their shape: very long average lines. Skipped, and retracted when
        // previously ingested.
        if looks_minified_bytes(&bytes) {
            summary.skipped += 1;
            filtered_paths.insert(rel.clone());
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
            Ok(()) => {
                if text.trim().is_empty() {
                    // Empty-file-fix: skip ingest (the chunker rejects empty
                    // text by contract); count as skipped and remember the
                    // hash for the manifest so the next scan is a no-op for
                    // this path. The prior-graph delete above already ran, so
                    // a file that shrank to empty retracts its old graph.
                    empty_rels.push((rel.clone(), hash));
                } else {
                    to_ingest.push(ReadyToIngest {
                        kind: *kind,
                        rel: rel.clone(),
                        content: bytes,
                        hash,
                    })
                }
            }
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
                                // Anchor name part — receiver-qualified (`Foo::bar`)
                                // for nested declarations. Emit BOTH the qualified
                                // form and the bare tail: AST callees are bare, so
                                // extract_calls matches the tail (RFC-0020 Phase 2).
                                .and_then(|a| a.split_once(' ').map(|x| x.1))
                                .map(|name| {
                                    let mut names = vec![name.to_owned()];
                                    if let Some(tail) = engram_code::bare_tail(name) {
                                        names.push(tail.to_owned());
                                    }
                                    names
                                })
                        })
                        .flatten()
                        .collect::<Vec<String>>(),
                )
            })
            .flatten()
            .collect(),
    );

    let name_index: Arc<Mutex<engram_code::SymbolIndex>> =
        Arc::new(Mutex::new(engram_code::SymbolIndex::new()));
    let outcomes: Vec<(String, Outcome, Vec<KnowledgeEntity>)> = to_ingest
        .par_iter()
        .map(|item| {
            let rel = &item.rel;
            let kind = item.kind;
            if item.content.len() as u64 > max_bytes {
                // Defense-in-depth TOCTOU guard (shouldn't fire; pre-pass
                // already checked).
                return (rel.clone(), Outcome::Skipped, Vec::new());
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
                source_metadata: git_source_metadata.clone(),
            };
            // Tree-sitter chunking for supported extensions; fallback to the
            // ingestor's internal chunker for others.
            let ext = rel.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
            // code-graph-quality [parse-multiplicity]: parse ONCE per file and
            // share the tree across chunking, call extraction, and structural
            // extraction (previously 3 independent parses here + 1 in the
            // pre-pass name collection — 4× the dominant scan CPU).
            let shared_tree = ts_chunker
                .as_ref()
                .filter(|ts| ts.supports(ext))
                .and_then(|ts| ts.parse(&text_for_ast, ext).ok());
            let ingested = if let (Some(ts), Some(tree)) = (ts_chunker.as_ref(), shared_tree.as_ref()) {
                let candidates = match ts.chunk_with_tree(tree, &text, ext) {
                    Ok(c) => c,
                    Err(_) => return (rel.clone(), Outcome::Error, Vec::new()),
                };
                let mut req = request;
                req.text = text;
                block_on(code_ingestor.ingest_with_candidates(repo, req, candidates))
            } else {
                let mut req = request;
                req.text = text;
                match kind {
                    FileKind::Code => block_on(code_ingestor.ingest(repo, req)),
                    FileKind::Text if is_markdown_ext(ext) => {
                        block_on(markdown_ingestor.ingest(repo, req))
                    }
                    FileKind::Text => block_on(text_ingestor.ingest(repo, req)),
                }
            };
            let ingested = match ingested {
                Ok(i) => i,
                Err(_) => return (rel.clone(), Outcome::Error, Vec::new()),
            };
            // AST-level call extraction using the GLOBAL entity name set (not
            // just this file's symbols). This preserves cross-file call edges.
            let ast_calls = if let (Some(ts), Some(tree)) = (ts_chunker.as_ref(), shared_tree.as_ref()) {
                // Run whenever the grammar supports the file — an empty
                // global name set yields empty calls, NOT a fallback to the
                // fact-less extract_into path (which silently dropped all
                // Phase-2 structural/framework facts for declaration-free
                // files; final-review sweep pin caught it).
                ts.extract_calls_tree(tree, &text_for_ast, ext, &global_entity_names)
                    .ok()
            } else {
                None
            };
            let frameworks = {
                let facts = engram_code::extract_frameworks(&text_for_ast, ext);
                if facts.is_empty() { None } else { Some(facts) }
            };
            let structural = if let (Some(ts), Some(tree)) = (ts_chunker.as_ref(), shared_tree.as_ref()) {
                ts.extract_structural_tree(tree, &text_for_ast, ext).ok()
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
                        structural.as_ref(),
                        frameworks.as_ref(),
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
                        let repo = ingested
                            .source
                            .metadata
                            .as_ref()
                            .and_then(|m| m.get(crate::source_key::REPOSITORY_KEY))
                            .and_then(|v| v.as_str());
                        let path = ingested.document.path.as_deref();
                        crate::extractor::register_entities(&mut idx, &g.entities, repo, path);
                        g.unresolved =
                            engram_code::resolve_refs(&idx, &mut g.relationships, repo, path);
                    }
                    // Persist the graph + entities + relationships + ledger.
                    let _ = block_on(async {
                        repo.put_graph(g.graph.clone()).await?;
                        for entity in &g.entities {
                            repo.put_entity(entity.clone()).await?;
                        }
                        for rel in &g.relationships {
                            repo.put_relationship(rel.clone()).await?;
                        }
                        if !g.unresolved.is_empty() {
                            if let Err(e) = repo.put_unresolved_refs(g.unresolved.clone()).await {
                                // Sibling pattern of the five warning-tagged
                                // scans below: surface, never swallow.
                                eprintln!(
                                    "[engram-ingest] warning: failed to persist ledger rows for '{rel}': {e}"
                                );
                            }
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
                        Err(_) => return (rel.clone(), Outcome::Error, Vec::new()),
                    }
                }
            };
            // Contract extraction (T4/T5/T6): for YAML/JSON files, attempt
            // OpenAPI detection and emit EntityKind::Api entities + exposes edges.
            // Uses `text_for_ast` (a pre-move clone of the file text) so we do
            // not need to read the content a second time.
            let (contract_keys, contract_parse_failed, contract_had_write_error) =
                contract_phase::extract_contract_entities(
                    repo,
                    &opts.scope,
                    &source_key,
                    &text_for_ast,
                    ext,
                    &ingested.source.provenance,
                );

            let emitted_entities = extracted.entities.clone();
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
                emitted_entities,
            )
        })
        .collect();

    // RFC-0020 T3: the doc↔code `describes` bridge is removed — non-code
    // documents no longer emit Concept entities, so there is nothing to bridge.
    // Document↔code association is computed at recall time (chunk lane matches
    // on symbol text/path), not stored as topology.

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
        .filter(|k| {
            !k.starts_with(CONTRACT_PREFIX)
                && (!observed_paths.contains(*k) || filtered_paths.contains(*k))
        })
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
    // (FIX 1 / adversarial Nit 6). A failed GC leaves a harmless orphan repo
    // node that converges on the next scan; surface it for observability
    // instead of swallowing it.
    if block_on(reconcile::maybe_delete_repo_node(
        repo,
        &opts.scope,
        &source_key,
    ))
    .is_err()
    {
        summary.errors += 1;
    }

    // Build the emitted manifest from this scan's outcomes.
    let mut new_manifest = std::collections::HashMap::new();

    // Accumulate contract keys emitted during the parallel phase so we can do
    // RFC-0020 Phase 2 orphan sweep: re-attempt every pending ledger
    // reference against the symbol table built from this scan's ingested
    // files. A reference whose target landed in a LATER scan resolves here
    // — the edge is written and the row flips to resolved, without
    // re-ingesting the referring file. (Unchanged files are not re-indexed
    // into this scan's table, so a sweep only heals rows whose target was
    // (re)defined in files ingested this pass.)
    {
        let pending =
            block_on(repo.list_unresolved_refs(&opts.scope, UnresolvedReferenceStatus::Pending))
                .unwrap_or_default();
        if !pending.is_empty() {
            let idx = name_index.lock().unwrap_or_else(|e| e.into_inner());
            for row in pending {
                let reference = row.reference_name.clone();
                let (hint, name) = engram_code::split_dotted(&reference);
                let mut outcome = idx.resolve(name, None, None);
                if !matches!(outcome, engram_code::Resolution::Resolved(_)) {
                    if let Some(hint) = hint {
                        let qualified =
                            format!("{}::{}", engram_code::receiver_type_hint(hint), name);
                        outcome = idx.resolve(&qualified, None, None);
                    }
                }
                if let engram_code::Resolution::Resolved(candidate) = outcome {
                    let scope = row.scope.clone();
                    let subject_id = row.from_entity_id.clone();
                    let object_id = Id::from(candidate.id.clone());
                    let graph_id = row.graph_id.clone();
                    // The healed edge keeps the reference's own shape: an
                    // Endpoint subject (a route) heals as `routes_to`,
                    // everything else as `calls` (the row carries no
                    // predicate; the kind comes from the subject entity).
                    let subject_entity =
                        block_on(repo.get_entity(&row.from_entity_id, &opts.scope))
                            .ok()
                            .flatten();
                    let subject_kind = subject_entity.as_ref().map(|entity| entity.kind.clone());
                    let predicate = match subject_kind {
                        Some(engram_domain::EntityKind::Endpoint) => "routes_to",
                        _ => "calls",
                    };
                    let rel = KnowledgeRelationship {
                        id: RelationshipId::from(format!("sweep-{subject_id}-{object_id}")),
                        graph_id,
                        subject: EntityRef {
                            id: Some(subject_id.clone()),
                            kind: None,
                            // Canonical names (code-graph-quality follow-up):
                            // healed edges previously carried an EMPTY subject
                            // name and the AS-WRITTEN bare object reference —
                            // invisible to name-keyed navigation. Stamp both
                            // endpoints with the resolved entities' names.
                            name: subject_entity.as_ref().map(|e| e.name.clone()),
                            aliases: Vec::new(),
                        },
                        predicate: predicate.to_owned(),
                        object: EntityRef {
                            id: Some(object_id.clone()),
                            kind: None,
                            name: Some(
                                idx.lookup(object_id.as_str())
                                    .map(|c| c.name.clone())
                                    .unwrap_or(reference.clone()),
                            ),
                            aliases: Vec::new(),
                        },
                        scope: scope.clone(),
                        evidence: Vec::new(),
                        confidence: Some(0.9),
                        provenance: Provenance {
                            source: "engram-orphans-sweep".to_owned(),
                            actor: opts.actor.clone(),
                            observed_at: chrono::Utc::now(),
                            evidence: Vec::new(),
                            derivations: Vec::new(),
                            confidence: None,
                            method: Some("deterministic_orphan_sweep".to_owned()),
                        },
                        created_at: chrono::Utc::now(),
                        updated_at: None,
                        archived_at: None,
                    };
                    let row_id = row.id.clone();
                    if (block_on(async {
                        repo.put_relationship(rel).await?;
                        repo.update_unresolved_status(
                            &row_id,
                            UnresolvedReferenceStatus::Resolved,
                            &scope,
                        )
                        .await?;
                        Ok::<(), CoreError>(())
                    }))
                    .is_err()
                    {
                        summary.errors += 1;
                    }
                }
            }
        }
    }

    // T8 retraction and update the contract manifest in the serial post-pass.
    let mut current_contract_ops: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    // Files whose contract entity/edge write failed this scan: their hash is not
    // recorded (reprocess next scan) and their prior declarations carry forward,
    // so a transient write failure cannot permanently retract a still-declared op.
    let mut write_error_rels: Vec<String> = Vec::new();

    for (rel, outcome, _entities) in outcomes {
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
        .filter(|(k, _)| k.starts_with(CONTRACT_PREFIX))
        .flat_map(|(_, v)| serde_json::from_str::<Vec<String>>(v).unwrap_or_default())
        .collect();

    // Start with freshly-parsed keys.
    let mut current_union: HashSet<String> = current_contract_ops
        .values()
        .flat_map(|keys| keys.iter().cloned())
        .collect();
    // Keys from unchanged files (still present and declared).
    for rel in &unchanged_rels {
        let mk = format!("{CONTRACT_PREFIX}{rel}");
        if let Some(json) = opts.manifest.get(&mk) {
            current_union.extend(serde_json::from_str::<Vec<String>>(json).unwrap_or_default());
        }
    }
    // Keys from files whose graph-delete failed (not re-processed this scan;
    // declarations carry forward until the retry succeeds).
    for rel in &delete_failed {
        let mk = format!("{CONTRACT_PREFIX}{rel}");
        if let Some(json) = opts.manifest.get(&mk) {
            current_union.extend(serde_json::from_str::<Vec<String>>(json).unwrap_or_default());
        }
    }
    // Keys from removed paths where delete failed (kept in manifest for retry).
    for rel in &removed_delete_failed {
        let mk = format!("{CONTRACT_PREFIX}{rel}");
        if let Some(json) = opts.manifest.get(&mk) {
            current_union.extend(serde_json::from_str::<Vec<String>>(json).unwrap_or_default());
        }
    }
    // Keys from files whose contract write failed this scan: not re-emitted, but
    // their prior declarations still stand (the file reprocesses next scan), so a
    // transient write failure cannot retract a still-declared op.
    for rel in &write_error_rels {
        let mk = format!("{CONTRACT_PREFIX}{rel}");
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
        let ck = format!("{CONTRACT_PREFIX}{rel}");
        if let Some(j) = opts.manifest.get(&ck) {
            new_manifest.insert(ck, j.clone());
        }
        progress(ScanProgress {
            file: rel,
            status: "unchanged",
        });
    }

    // Empty-file-fix: count trim-empty files as skipped and carry their
    // hashes into the manifest so the next scan treats them as unchanged
    // (zero retraction work). They are NOT indexed — there is no content.
    for (rel, hash) in empty_rels {
        summary.skipped += 1;
        new_manifest.insert(rel.clone(), hash);
        progress(ScanProgress {
            file: rel,
            status: "skipped",
        });
    }

    // Emit contract manifest entries for files processed in this scan.
    for (rel, keys) in &current_contract_ops {
        if !keys.is_empty() {
            if let Ok(json) = serde_json::to_string(keys) {
                new_manifest.insert(format!("{CONTRACT_PREFIX}{rel}"), json);
            }
        }
    }

    // Carry forward the prior contract manifest entry for write-error files (their
    // file hash was not recorded, so they reprocess next scan; keep the prior
    // declaration meanwhile so prior_union stays consistent).
    for rel in &write_error_rels {
        let ck = format!("{CONTRACT_PREFIX}{rel}");
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
        let ck = format!("{CONTRACT_PREFIX}{rel}");
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
        let ck = format!("{CONTRACT_PREFIX}{rel}");
        if let Some(j) = opts.manifest.get(&ck) {
            new_manifest.insert(ck, j.clone());
        }
    }

    Ok((summary, new_manifest))
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
