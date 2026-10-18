//! Scan phase 5: serial manifest assembly — outcome folding into the summary,
//! the T8 per-source contract retraction, and every manifest carry-forward
//! rule (unchanged, empty, write-error, delete-failed, removed-failed).
//!
//! Relocated verbatim from `scan_repository` (scanner god-module split);
//! behavior unchanged — the scanner suite is the regression net.

use std::collections::{HashMap, HashSet};

use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository};
use futures::executor::block_on;

use super::{CONTRACT_PREFIX, Outcome, ScanOptions, ScanProgress, ScanSummary};
use crate::contract;

/// Fold the parallel-phase outcomes into `summary` + the emitted manifest,
/// run the T8 per-source contract retraction, and apply every carry-forward
/// rule. `progress` receives per-file status callbacks.
#[allow(clippy::too_many_arguments)]
pub(super) fn assemble_manifest<R>(
    repo: &R,
    opts: &ScanOptions,
    outcomes: Vec<(String, Outcome, Vec<engram_domain::KnowledgeEntity>)>,
    unchanged_rels: Vec<String>,
    empty_rels: Vec<(String, String)>,
    delete_failed: &HashSet<String>,
    removed_delete_failed: &HashSet<String>,
    source_key: &str,
    progress: &dyn Fn(ScanProgress),
) -> (ScanSummary, HashMap<String, String>)
where
    R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync,
{
    let mut summary = ScanSummary::default();
    let mut new_manifest: HashMap<String, String> = HashMap::new();
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
    for rel in delete_failed {
        let mk = format!("{CONTRACT_PREFIX}{rel}");
        if let Some(json) = opts.manifest.get(&mk) {
            current_union.extend(serde_json::from_str::<Vec<String>>(json).unwrap_or_default());
        }
    }
    // Keys from removed paths where delete failed (kept in manifest for retry).
    for rel in removed_delete_failed {
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
            source_key,
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
    for rel in delete_failed {
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
    for rel in removed_delete_failed {
        if let Some(h) = opts.manifest.get(rel.as_str()) {
            new_manifest.insert(rel.clone(), h.clone());
        }
        // Also carry forward contract manifest entries for these files.
        let ck = format!("{CONTRACT_PREFIX}{rel}");
        if let Some(j) = opts.manifest.get(&ck) {
            new_manifest.insert(ck, j.clone());
        }
    }

    (summary, new_manifest)
}
