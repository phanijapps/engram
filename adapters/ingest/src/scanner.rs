//! Parallel repository scanner (RFC 0004 background-repo-indexer).
//!
//! Walks a root `.gitignore`-aware, applies the Slice-1 data-custody controls
//! (path confinement, secret-file blocklist, deny list, size bound), then ingests
//! the readable files in parallel with rayon. The pure filter helpers are unit-
//! tested; the security controls are ported from `demo/backend/src/decide.ts`
//! and must not be relaxed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult, KnowledgeGraphRepository, KnowledgeRepository};
use futures::executor::block_on;
use rayon::prelude::*;

use crate::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
    PlainTextChunker, PlainTextChunkerOptions, content_hash, reconcile, stable_source_key,
};

const DEFAULT_MAX_BYTES: u64 = 1024 * 1024; // 1 MiB per file

const DENY_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "coverage",
    ".fastembed_cache",
    "__pycache__",
    ".venv",
    "venv",
    ".next",
    ".cache",
    ".idea",
    ".vscode",
];
const DENY_FILE_EXT: &[&str] = &["db", "sqlite", "sqlite3", "node", "log", "pyc", "lock"];
const SECRET_EXT: &[&str] = &[".key", ".pem", ".cert", ".crt", ".p12", ".pfx"];
const SECRET_NAMES: &[&str] = &["id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"];
const SAFE_TEMPLATES: &[&str] = &[
    ".env.example",
    ".env.sample",
    ".env.template",
    ".env.defaults",
    ".env.schema",
];
const CODE_NAMES: &[&str] = &[
    "dockerfile",
    "makefile",
    "rakefile",
    "gemfile",
    "cmake",
    "justfile",
];
const CODE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "kts", "scala", "clj",
    "cljs", "ex", "exs", "erl", "hs", "ml", "mli", "lua", "php", "pl", "pm", "r", "rb", "sh",
    "bash", "zsh", "fish", "ps1", "c", "h", "cpp", "cc", "cxx", "hpp", "hxx", "cs", "swift",
    "dart", "vue", "svelte", "sql", "proto", "graphql", "gradle", "groovy", "vim",
];
const TEXT_EXTENSIONS: &[&str] = &[
    "md",
    "markdown",
    "txt",
    "rst",
    "org",
    "tex",
    "adoc",
    "yml",
    "yaml",
    "json",
    "toml",
    "xml",
    "html",
    "htm",
    "css",
    "scss",
    "sass",
    "less",
    "ini",
    "cfg",
    "conf",
    "properties",
    "csv",
    "tsv",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Code,
    Text,
}

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

/// Detect git metadata (remote URL, branch, short SHA) if the root is a git repo.
fn detect_git(root: &Path) -> Option<(String, String, String)> {
    let run = |args: &[&str]| -> Option<String> {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if s.is_empty() { None } else { Some(s) }
    };
    let remote = run(&["remote", "get-url", "origin"])?;
    let branch = run(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    let sha = run(&["rev-parse", "--short=10", "HEAD"])?;
    Some((remote, branch, sha))
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

fn file_base(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// True if `target` is `root` or inside it (callers canonicalize both first).
pub fn is_within_root(target: &Path, root: &Path) -> bool {
    target.starts_with(root)
}

/// True if any path segment is a deny dir, or the file suffix is denylisted.
pub fn is_denylisted(rel_path: &str) -> bool {
    let segs: Vec<&str> = rel_path.split(['/', '\\']).collect();
    if segs.iter().any(|s| DENY_DIRS.contains(s)) {
        return true;
    }
    let base = segs.last().copied().unwrap_or("");
    let ext = match base.rsplit_once('.') {
        Some((_, e)) => e.to_lowercase(),
        None => String::new(),
    };
    DENY_FILE_EXT.iter().any(|e| *e == ext)
}

/// True if the file name looks like a credential/secret carrier.
pub fn is_secret_file(name: &str) -> bool {
    let base = file_base(name).to_lowercase();
    if SAFE_TEMPLATES.iter().any(|t| *t == base) {
        return false;
    }
    if base == ".env" || base.starts_with(".env.") {
        return true;
    }
    if SECRET_EXT.iter().any(|e| base.ends_with(e)) {
        return true;
    }
    SECRET_NAMES.iter().any(|n| *n == base)
}

/// Classify a file by name; `None` means "not included".
pub fn classify_file(name: &str) -> Option<FileKind> {
    let base = file_base(name).to_lowercase();
    if CODE_NAMES.iter().any(|n| *n == base) {
        return Some(FileKind::Code);
    }
    let ext = match base.rsplit_once('.') {
        Some((_, e)) => e,
        None => "",
    };
    if CODE_EXTENSIONS.contains(&ext) {
        return Some(FileKind::Code);
    }
    if TEXT_EXTENSIONS.contains(&ext) {
        return Some(FileKind::Text);
    }
    None
}

#[derive(Debug)]
enum Outcome {
    Ingested {
        entities: usize,
        relationships: usize,
        hash: (String, String),
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
            // Extract entity names from chunk anchors for AST call matching.
            let entity_names: HashSet<String> = ingested
                .chunks
                .iter()
                .filter_map(|c| {
                    c.location
                        .as_ref()
                        .and_then(|l| l.anchor.as_deref())
                        .and_then(|a| a.split_once(' ').map(|x| x.1).map(|s| s.to_owned()))
                })
                .collect();
            // AST-level call extraction when tree-sitter supports the extension.
            let ast_calls = if let Some(ref ts) = ts_chunker {
                if ts.supports(ext) && !entity_names.is_empty() {
                    ts.extract_calls(&text_for_ast, ext, &entity_names).ok()
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
                Ok(g) => {
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
                Err(_) => match block_on(extractor.extract_into(
                    repo,
                    &ingested.source,
                    &ingested.document,
                    &ingested.chunks,
                )) {
                    Ok(e) => e,
                    Err(_) => return (rel.clone(), Outcome::Error),
                },
            };
            (
                rel.clone(),
                Outcome::Ingested {
                    entities: extracted.entities.len(),
                    relationships: extracted.relationships.len(),
                    hash: (rel.clone(), hash),
                },
            )
        })
        .collect();

    // FIX 1(c): Serial post-pass — delete graphs for paths that were in the
    // prior manifest but were never observed during this scan (genuinely-absent
    // files).  Uses `observed_paths` (FIX 3) rather than `readable` so that
    // oversize / denylisted / unclassifiable files that are still present on
    // disk are not treated as removals.
    let removed_paths: Vec<String> = opts
        .manifest
        .keys()
        .filter(|k| !observed_paths.contains(*k))
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

    for (rel, outcome) in outcomes {
        match outcome {
            Outcome::Ingested {
                entities,
                relationships,
                hash: (r, h),
            } => {
                summary.ingested += 1;
                summary.entities += entities;
                summary.relationships += relationships;
                new_manifest.insert(r, h);
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

    // Carry forward hashes for files whose content did not change this scan.
    for rel in unchanged_rels {
        summary.unchanged += 1;
        if let Some(h) = opts.manifest.get(&rel) {
            new_manifest.insert(rel.clone(), h.clone());
        }
        progress(ScanProgress {
            file: rel,
            status: "unchanged",
        });
    }

    // FIX 2: Files where the pre-pass delete failed — keep the old hash so
    // the path is retried next scan.  The prior graph was NOT deleted, so no
    // duplicate graph exists and the state is consistent.
    for rel in &delete_failed {
        if let Some(h) = opts.manifest.get(rel.as_str()) {
            new_manifest.insert(rel.clone(), h.clone());
        }
    }

    // FIX 2: Removed paths where the post-pass delete failed — keep in the
    // manifest so they are retried rather than being silently pruned.
    for rel in &removed_delete_failed {
        if let Some(h) = opts.manifest.get(rel.as_str()) {
            new_manifest.insert(rel.clone(), h.clone());
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
    fn denylisted_dirs_and_extensions() {
        assert!(is_denylisted("node_modules/x.js"));
        assert!(is_denylisted("a/.git/config"));
        assert!(is_denylisted("data.db"));
        assert!(is_denylisted("lockfile.lock"));
        assert!(!is_denylisted("src/main.rs"));
        assert!(!is_denylisted("README.md"));
    }

    #[test]
    fn secret_files_detected_but_templates_safe() {
        assert!(is_secret_file(".env"));
        assert!(is_secret_file(".env.local"));
        assert!(is_secret_file("id_rsa"));
        assert!(is_secret_file("certs/server.pem"));
        assert!(is_secret_file("key.crt"));
        // Templates document variables without secrets — not flagged.
        assert!(!is_secret_file(".env.example"));
        assert!(!is_secret_file(".env.sample"));
        assert!(!is_secret_file("src/main.rs"));
    }

    #[test]
    fn classify_by_name_and_extension() {
        assert_eq!(classify_file("main.rs"), Some(FileKind::Code));
        assert_eq!(classify_file("Dockerfile"), Some(FileKind::Code));
        assert_eq!(classify_file("README.md"), Some(FileKind::Text));
        assert_eq!(classify_file("config.yaml"), Some(FileKind::Text));
        assert_eq!(classify_file("binary"), None);
    }
}
