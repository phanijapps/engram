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

use crate::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
    MarkdownChunker, PlainTextChunker, PlainTextChunkerOptions, git_detect::detect_git,
    qualified_source_key, scan_filter::ScanFilter, stable_source_key,
};

mod contract_phase;
mod ingest_phase;
mod manifest;
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
    /// Files whose chunked bytes cover less than half the file (large files
    /// only) — the fetch-content single-chunk class made visible at scan
    /// time instead of silently at retrieval time. Bounded to the first
    /// [`MAX_COVERAGE_WARNINGS`] offenders.
    pub coverage_warnings: Vec<String>,
}

/// Cap on `coverage_warnings` so a pathological scan cannot flood the summary.
const MAX_COVERAGE_WARNINGS: usize = 20;
/// Files below this size are exempt — tiny files legitimately chunk sparsely.
const COVERAGE_MIN_FILE_BYTES: usize = 8 * 1024;
/// Coverage ratio below which a large file is flagged.
const COVERAGE_MIN_RATIO: f64 = 0.5;

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
    let (repository_identity, git_ref_identity) = {
        let remote = git.as_ref().map(|(r, _, _)| r.as_str());
        let git_ref = git.as_ref().map(|(_, b, _)| b.as_str());
        let repo_key = stable_source_key(remote, &opts.source_name);
        (Some(repo_key.clone()), git_ref.map(str::to_owned))
    };
    // Branch-qualified (ADR-0032): when a ref is known, the partition
    // key becomes 'repo@ref' — a dev scan never retracts main's graphs.
    // No ref = bare repo key (backward-compatible with existing stores).
    let source_key = qualified_source_key(
        &repository_identity.clone().unwrap_or_default(),
        git_ref_identity.as_deref(),
    );

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

    let code_chunker = CodeSymbolChunker;
    let code_ingestor = KnowledgeIngestor::new(code_chunker);
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
    let name_index: Arc<Mutex<engram_code::SymbolIndex>> =
        Arc::new(Mutex::new(engram_code::SymbolIndex::new()));
    let global_entity_names: Arc<HashSet<String>> = {
        let mut idx = name_index.lock().unwrap_or_else(|e| e.into_inner());
        let seed = DocumentIngestRequest {
            source_kind: doc_source_kind.clone(),
            source_name: source_name.clone(),
            scope: opts.scope.clone(),
            document_kind: engram_domain::SourceDocumentKind::Code,
            document: DocumentMetadata::default(),
            text: String::new(),
            policy: opts.policy.clone(),
            actor: opts.actor.clone(),
            stable_source_key: Some(source_key.clone()),
            repository_key: repository_identity.clone(),
            git_ref: git_ref_identity.clone(),
            source_metadata: None,
        };
        Arc::new(prepass::collect_global_symbols(
            &to_ingest,
            ts_chunker.as_ref(),
            &mut idx,
            &seed,
        ))
    };
    let ingest_results = ingest_phase::parallel_ingest(
        &ingest_phase::IngestCtx {
            repo,
            opts,
            source_name: &source_name,
            doc_source_kind,
            source_key: &source_key,
            git_source_metadata: &git_source_metadata,
            code_ingestor: &code_ingestor,
            code_chunker: &code_chunker,
            markdown_ingestor: &markdown_ingestor,
            text_ingestor: &text_ingestor,
            ts_chunker: ts_chunker.as_ref(),
            extractor: &extractor,
            global_entity_names: &global_entity_names,
            name_index: &name_index,
        },
        &to_ingest,
    );
    let outcomes = ingest_results.outcomes;
    summary.coverage_warnings = ingest_results.coverage_warnings;

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

    // Orphan-ledger healing (phase 4b — scanner/sweep.rs).
    {
        let idx = name_index.lock().unwrap_or_else(|e| e.into_inner());
        sweep::heal_pending_refs(repo, opts, &idx, &mut summary);
    }

    // Manifest assembly (phase 5 — scanner/manifest.rs).
    let (folded, new_manifest) = manifest::assemble_manifest(
        repo,
        opts,
        outcomes,
        unchanged_rels,
        empty_rels,
        &delete_failed,
        &removed_delete_failed,
        &source_key,
        &progress,
    );
    summary.ingested += folded.ingested;
    summary.unchanged += folded.unchanged;
    summary.skipped += folded.skipped;
    summary.errors += folded.errors;
    summary.entities += folded.entities;
    summary.relationships += folded.relationships;

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
