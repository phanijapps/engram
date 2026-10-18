//! Parallel repository scanner (RFC 0004 background-repo-indexer).
//!
//! Walks a root `.gitignore`-aware, applies the Slice-1 data-custody controls
//! (path confinement, secret-file blocklist, deny list, size bound), then ingests
//! the readable files in parallel with rayon. The pure filter helpers are unit-
//! tested; the security controls are ported from `demo/backend/src/decide.ts`
//! and must not be relaxed.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult, KnowledgeGraphRepository, KnowledgeRepository};
use futures::executor::block_on;

use crate::{
    CodeSymbolChunker, GraphExtractor, KnowledgeIngestor, MarkdownChunker, PlainTextChunker,
    PlainTextChunkerOptions, contract, git_detect::detect_git, scan_filter::ScanFilter,
    stable_source_key,
};

mod contract_phase;
mod ingest_phase;
mod prepass;
mod sweep;
mod walk;
mod workspace;

pub use workspace::{detect_workspace, scan_workspace};

/// `true` for Markdown extensions routed through the structure-aware chunker.
fn is_markdown_ext(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown")
}

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

    // Walk + filter (phase 1 — see scanner/walk.rs).
    let walk::WalkOutput {
        readable,
        observed_paths,
        mut filtered_paths,
    } = walk::walk_and_classify(&root_canonical, opts, &mut summary)?;

    // Serial pre-pass (phase 2 — see scanner/prepass.rs).
    let prepass::PrepassOutput {
        to_ingest,
        unchanged_rels,
        empty_rels,
        delete_failed,
    } = prepass::serial_prepass(
        repo,
        opts,
        &source_key,
        &readable,
        &mut filtered_paths,
        &mut summary,
    );
    let global_entity_names: Arc<HashSet<String>> = Arc::new(prepass::collect_global_entity_names(
        &to_ingest,
        ts_chunker.as_ref(),
    ));

    let name_index: Arc<Mutex<engram_code::SymbolIndex>> =
        Arc::new(Mutex::new(engram_code::SymbolIndex::new()));
    let outcomes = ingest_phase::parallel_ingest(
        &ingest_phase::IngestCtx {
            repo,
            opts,
            source_name: &source_name,
            doc_source_kind,
            source_key: &source_key,
            git_source_metadata: &git_source_metadata,
            code_ingestor: &code_ingestor,
            markdown_ingestor: &markdown_ingestor,
            text_ingestor: &text_ingestor,
            ts_chunker: ts_chunker.as_ref(),
            extractor: &extractor,
            global_entity_names: &global_entity_names,
            name_index: &name_index,
        },
        &to_ingest,
    );

    // RFC-0020 T3: the doc↔code `describes` bridge is removed — non-code
    // documents no longer emit Concept entities, so there is nothing to bridge.
    // Document↔code association is computed at recall time (chunk lane matches
    // on symbol text/path), not stored as topology.

    // Post-pass sweeps (phase 4 — see scanner/sweep.rs).
    let removed_delete_failed = sweep::retract_removed_paths(
        repo,
        opts,
        &source_key,
        &observed_paths,
        &filtered_paths,
        &mut summary,
    );

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
    use crate::classifier::is_within_root;

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
