//! Parallel repository scanner (RFC 0004 background-repo-indexer).
//!
//! Walks a root `.gitignore`-aware, applies the Slice-1 data-custody controls
//! (path confinement, secret-file blocklist, deny list, size bound), then ingests
//! the readable files in parallel with rayon. The pure filter helpers are unit-
//! tested; the security controls are ported from `demo/backend/src/decide.ts`
//! and must not be relaxed.

use std::path::{Path, PathBuf};

use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult, KnowledgeGraphRepository, KnowledgeRepository};
use futures::executor::block_on;
use rayon::prelude::*;

use crate::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
    PlainTextChunker, PlainTextChunkerOptions, content_hash,
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
#[derive(Debug, Clone, Default)]
pub struct ScanSummary {
    pub scanned: usize,
    pub ingested: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub entities: usize,
    pub relationships: usize,
    pub errors: usize,
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
    if CODE_EXTENSIONS.iter().any(|e| *e == ext) {
        return Some(FileKind::Code);
    }
    if TEXT_EXTENSIONS.iter().any(|e| *e == ext) {
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
    Unchanged,
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

    let code_ingestor = KnowledgeIngestor::new(CodeSymbolChunker);
    let text_ingestor =
        KnowledgeIngestor::new(PlainTextChunker::new(PlainTextChunkerOptions::default())?);
    let extractor = GraphExtractor::new();

    // Walk + filter (sequential).
    let walker = ignore::WalkBuilder::new(&root_canonical)
        .follow_links(false)
        .build();
    let mut readable: Vec<(PathBuf, FileKind, String)> = Vec::new();
    let mut summary = ScanSummary::default();
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

    // Parallel ingest (rayon).
    let max_bytes = opts.max_bytes();
    let outcomes: Vec<(String, Outcome)> = readable
        .par_iter()
        .map(|(path, kind, rel)| {
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(_) => return (rel.clone(), Outcome::Error),
            };
            if bytes.len() as u64 > max_bytes {
                return (rel.clone(), Outcome::Skipped);
            }
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let hash = content_hash(&text);
            if opts.manifest.get(rel).is_some_and(|h| h == &hash) {
                return (rel.clone(), Outcome::Unchanged);
            }
            let document_kind = match kind {
                FileKind::Code => SourceDocumentKind::Code,
                FileKind::Text => SourceDocumentKind::Text,
            };
            let request = DocumentIngestRequest {
                source_kind: SourceKind::Filesystem,
                source_name: opts.source_name.clone(),
                scope: opts.scope.clone(),
                document_kind,
                document: DocumentMetadata {
                    path: Some(rel.clone()),
                    ..Default::default()
                },
                text,
                policy: opts.policy.clone(),
                actor: opts.actor.clone(),
            };
            let ingested = match kind {
                FileKind::Code => block_on(code_ingestor.ingest(repo, request)),
                FileKind::Text => block_on(text_ingestor.ingest(repo, request)),
            };
            let ingested = match ingested {
                Ok(i) => i,
                Err(_) => return (rel.clone(), Outcome::Error),
            };
            let extracted = match block_on(extractor.extract_into(
                repo,
                &ingested.source,
                &ingested.document,
                &ingested.chunks,
            )) {
                Ok(e) => e,
                Err(_) => return (rel.clone(), Outcome::Error),
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

    let mut new_manifest = opts.manifest.clone();
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
            Outcome::Unchanged => {
                summary.unchanged += 1;
                progress(ScanProgress {
                    file: rel,
                    status: "unchanged",
                });
            }
            Outcome::Skipped => {
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
