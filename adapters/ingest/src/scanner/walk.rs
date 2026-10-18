//! Scan phase 1: filesystem walk, path confinement, classification, and the
//! walk-time filters (denylist, secrets, minified assets, size bound).
//!
//! Relocated verbatim from `scan_repository` (scanner god-module split,
//! self-index finding \"scanner-god-module\"); behavior is unchanged — the
//! 36-test scanner suite is the regression net.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::classifier::{
    FileKind, classify_file, is_secret_file, is_within_root, looks_minified_name,
};

use engram_knowledge::{CoreError, CoreResult};

use super::ScanOptions;

/// A file that survived the walk filters: canonical path, classified kind,
/// repo-relative path.
pub(super) struct Readable {
    pub canonical: PathBuf,
    pub kind: FileKind,
    pub rel: String,
}

/// Walk output: readable files + bookkeeping sets the later phases consume.
pub struct WalkOutput {
    pub readable: Vec<Readable>,
    /// Every rel path observed during the walk — including filtered ones —
    /// so removed-path detection never mistakes a present-but-filtered file
    /// for a removal (FIX 3 / adversarial Concern 4).
    pub observed_paths: HashSet<String>,
    /// Paths filtered THIS scan by the minified-asset rules (name or content);
    /// a previously-ingested path here is retracted by the post-pass.
    pub filtered_paths: HashSet<String>,
}

pub(super) fn walk_and_classify(
    root_canonical_ref: &Path,
    opts: &ScanOptions,
    summary: &mut super::ScanSummary,
) -> CoreResult<WalkOutput> {
    let mut readable: Vec<Readable> = Vec::new();
    let mut observed_paths: HashSet<String> = HashSet::new();
    let mut filtered_paths: HashSet<String> = HashSet::new();

    // Walk + filter (sequential).
    let walker = ignore::WalkBuilder::new(root_canonical_ref)
        .follow_links(false)
        .build();
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
                if let Ok(rel) = path.strip_prefix(root_canonical_ref) {
                    let rel = rel.to_string_lossy().to_string();
                    if !rel.is_empty() && opts.manifest.contains_key(&rel) {
                        observed_paths.insert(rel);
                    }
                }
                summary.skipped += 1;
                continue;
            }
        };
        if !is_within_root(&canonical, root_canonical_ref) {
            summary.skipped += 1;
            continue;
        }
        let rel = canonical
            .strip_prefix(root_canonical_ref)
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
        readable.push(Readable {
            canonical,
            kind,
            rel,
        });
    }

    Ok(WalkOutput {
        readable,
        observed_paths,
        filtered_paths,
    })
}
