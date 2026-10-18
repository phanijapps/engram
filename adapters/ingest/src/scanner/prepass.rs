//! Scan phase 2: serial pre-pass — read + hash each walked file, split
//! unchanged vs changed, delete prior graphs for changed paths (capturing
//! temporal validity stamps), and collect the global entity-name set for
//! cross-file AST call resolution.
//!
//! Relocated verbatim from `scan_repository` (scanner god-module split);
//! behavior unchanged — the scanner suite is the regression net.

use std::collections::HashSet;

use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository};
use futures::executor::block_on;
use rayon::prelude::*;

use crate::classifier::looks_minified_bytes;
use crate::content_hash;
use crate::reconcile::{self, PriorValidity};

use super::walk::Readable;
use super::{ScanOptions, ScanSummary};

/// A file queued for the parallel ingest phase: bytes already read (never
/// read twice), content hash, and the prior graph's validity stamps for
/// temporal carry-over.
pub(super) struct ReadyToIngest {
    pub kind: crate::classifier::FileKind,
    pub rel: String,
    pub content: Vec<u8>,
    pub hash: String,
    /// Prior-graph validity stamps ("<kind>::<name>" -> validFrom) for
    /// unchanged-symbol carry-over (temporal discrimination).
    pub prior_validity: PriorValidity,
}

pub(super) struct PrepassOutput {
    pub to_ingest: Vec<ReadyToIngest>,
    pub unchanged_rels: Vec<String>,
    /// Trim-empty files (0-byte or whitespace-only): nothing to index, but the
    /// path still retracts + records its hash so later scans treat it as
    /// unchanged (empty-file-fix).
    pub empty_rels: Vec<(String, String)>,
    pub delete_failed: HashSet<String>,
}

pub(super) fn serial_prepass<R>(
    repo: &R,
    opts: &ScanOptions,
    source_key: &str,
    readable: &[Readable],
    filtered_paths: &mut HashSet<String>,
    summary: &mut ScanSummary,
) -> PrepassOutput
where
    R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync,
{
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
    let mut unchanged_rels: Vec<String> = Vec::new();
    let mut delete_failed: HashSet<String> = HashSet::new();
    let mut to_ingest: Vec<ReadyToIngest> = Vec::new();
    // Trim-empty files (0-byte or whitespace-only): nothing to index, but the
    // path still goes through prior-graph retraction + manifest recording so
    // a file that BECAME empty retracts cleanly and later scans treat it as
    // unchanged. Previously these reached the chunker, which rejects empty
    // text by contract — surfacing as scan `errors` (empty-file-fix).
    let mut empty_rels: Vec<(String, String)> = Vec::new();

    for r in readable {
        let (canonical, kind, rel) = (&r.canonical, &r.kind, &r.rel);
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
            source_key,
            rel,
        )) {
            Ok(prior_validity) => {
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
                        prior_validity,
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

    PrepassOutput {
        to_ingest,
        unchanged_rels,
        empty_rels,
        delete_failed,
    }
}

/// Global entity-name pre-collection: every tree-sitter-supported file's
/// symbol names (qualified + bare tail) so AST call extraction can match
/// cross-file callees (RFC-0020 Phase 2).
pub(super) fn collect_global_entity_names(
    to_ingest: &[ReadyToIngest],
    ts_chunker: Option<&crate::TreeSitterChunker>,
) -> HashSet<String> {
    // Pre-pass: collect ALL entity names globally so AST call extraction can
    // match cross-file callees. Without this, extract_calls only knows about
    // symbols declared in the current file, dropping most cross-file edges.
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
        .collect()
}
