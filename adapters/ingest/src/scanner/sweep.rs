//! Scan phase 4: serial post-pass sweeps — removed-path retraction, repo-node
//! GC, and the RFC-0020 Phase 2 orphan-ledger healing.
//!
//! Relocated verbatim from `scan_repository` (scanner god-module split);
//! behavior unchanged — the scanner suite is the regression net.

use std::collections::HashSet;

use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository};
use futures::executor::block_on;

use super::CONTRACT_PREFIX;
use super::{ScanOptions, ScanSummary};
use crate::reconcile;

/// FIX 1(c) + FIX 3: delete graphs for prior-manifest paths that were never
/// observed this scan (genuinely-absent files) or were filtered by upgraded
/// minified-asset rules; then GC the per-source Repository node once, after
/// all deletions complete.
pub(super) fn retract_removed_paths<R>(
    repo: &R,
    opts: &ScanOptions,
    source_key: &str,
    observed_paths: &HashSet<String>,
    filtered_paths: &HashSet<String>,
    summary: &mut ScanSummary,
) -> HashSet<String>
where
    R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync,
{
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
            source_key,
            removed_path,
        )) {
            Ok(_prior_validity_of_removed_path) => {}
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
        source_key,
    ))
    .is_err()
    {
        summary.errors += 1;
    }

    removed_delete_failed
}
