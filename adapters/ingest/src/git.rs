//! Local Git worktree source reader.
//!
//! This module owns Git CLI interaction for tracked-file discovery. It stays in
//! `engram-ingest` so core crates remain free of process execution, repository
//! state, remotes, branches, and history concerns.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::{KnowledgeSource, SourceDocument, SourceKind, SourceLocation};
use engram_knowledge::{CoreError, CoreResult, SourceReader};

use crate::{
    content_hash,
    filesystem::{
        FilesystemSourceReaderOptions, classify_document, file_error, language, mime_type,
        read_file_bytes, source_document_id, validate_relative_path,
    },
};

/// Source reader for tracked files in a local Git worktree.
///
/// The reader uses `git ls-files` for discovery and reuses filesystem safety
/// checks for content reads. It does not clone repositories, inspect history, or
/// include untracked files.
#[derive(Debug, Clone)]
pub struct GitSourceReader {
    root: PathBuf,
    options: FilesystemSourceReaderOptions,
}

impl GitSourceReader {
    /// Creates a Git source reader rooted at a local worktree.
    ///
    /// Construction verifies that `git` recognizes the root as part of a
    /// worktree. Tests and local tools can use an unborn repository; commits are
    /// not required for tracked-file discovery.
    pub fn new(
        root: impl Into<PathBuf>,
        options: FilesystemSourceReaderOptions,
    ) -> CoreResult<Self> {
        let root = root.into();
        if options.max_file_bytes == 0 {
            return invalid("max_file_bytes must be greater than zero");
        }
        let metadata = std::fs::metadata(&root).map_err(file_error)?;
        if !metadata.is_dir() {
            return invalid("git source root must be a directory");
        }
        run_git(&root, ["rev-parse", "--is-inside-work-tree"])?;
        Ok(Self { root, options })
    }

    fn tracked_paths(&self) -> CoreResult<Vec<String>> {
        let output = run_git_bytes(&self.root, ["ls-files", "-z"])?;
        let mut paths = output
            .split(|byte| *byte == 0)
            .filter(|bytes| !bytes.is_empty())
            .map(|bytes| {
                String::from_utf8(bytes.to_vec()).map_err(|error| CoreError::InvalidRequest {
                    reason: format!("tracked path is not valid UTF-8: {error}"),
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        paths.sort();
        Ok(paths)
    }

    fn tracked_path_set(&self) -> CoreResult<BTreeSet<String>> {
        self.tracked_paths()
            .map(Vec::into_iter)
            .map(Iterator::collect)
    }

    fn head_revision(&self) -> Option<String> {
        run_git(&self.root, ["rev-parse", "--verify", "HEAD"]).ok()
    }

    fn document_for_path(
        &self,
        source: &KnowledgeSource,
        relative_path: &str,
        head_revision: Option<&str>,
    ) -> CoreResult<SourceDocument> {
        validate_relative_path(relative_path)?;
        let path = Path::new(relative_path);
        let kind = classify_document(path).expect("classified before document construction");
        let bytes = read_file_bytes(&self.root, &self.options, relative_path)?;
        let content_hash = content_hash(&bytes);
        let now = Utc::now();

        Ok(SourceDocument {
            id: source_document_id(&source.id.to_string(), relative_path, &content_hash),
            source_id: source.id.clone(),
            kind,
            uri: None,
            path: Some(relative_path.to_owned()),
            title: path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned),
            mime_type: mime_type(path).map(str::to_owned),
            language: language(path).map(str::to_owned),
            version: source
                .version
                .clone()
                .or_else(|| head_revision.map(str::to_owned)),
            content_hash,
            provenance: source.provenance.clone(),
            policy: source.policy.clone(),
            created_at: now,
            updated_at: None,
            metadata: Some(
                [(
                    "location".to_owned(),
                    serde_json::to_value(SourceLocation {
                        path: Some(relative_path.to_owned()),
                        start_line: None,
                        end_line: None,
                        start_offset: None,
                        end_offset: None,
                        anchor: None,
                    })
                    .map_err(|error| CoreError::Adapter {
                        adapter: "engram-ingest-git".to_owned(),
                        message: error.to_string(),
                    })?,
                )]
                .into_iter()
                .collect(),
            ),
        })
    }
}

#[async_trait]
impl SourceReader for GitSourceReader {
    async fn read_source(&self, source: &KnowledgeSource) -> CoreResult<Vec<SourceDocument>> {
        if source.kind != SourceKind::GitRepository {
            return invalid("git reader requires SourceKind::GitRepository");
        }
        let head_revision = self.head_revision();
        self.tracked_paths()?
            .into_iter()
            .filter(|path| classify_document(Path::new(path)).is_some())
            .map(|path| self.document_for_path(source, &path, head_revision.as_deref()))
            .collect()
    }

    async fn read_document(&self, document: &SourceDocument) -> CoreResult<String> {
        let path = document
            .path
            .as_deref()
            .ok_or_else(|| CoreError::InvalidRequest {
                reason: "document.path is required for git reads".to_owned(),
            })?;
        validate_relative_path(path)?;
        if !self.tracked_path_set()?.contains(path) {
            return invalid("document path is not tracked by git");
        }
        let bytes = read_file_bytes(&self.root, &self.options, path)?;
        String::from_utf8(bytes).map_err(|error| CoreError::InvalidRequest {
            reason: format!("document is not valid UTF-8: {error}"),
        })
    }
}

fn run_git<const N: usize>(root: &Path, args: [&str; N]) -> CoreResult<String> {
    let output = run_git_bytes(root, args)?;
    String::from_utf8(output)
        .map(|value| value.trim().to_owned())
        .map_err(|error| CoreError::Adapter {
            adapter: "engram-ingest-git".to_owned(),
            message: error.to_string(),
        })
}

fn run_git_bytes<const N: usize>(root: &Path, args: [&str; N]) -> CoreResult<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| CoreError::Adapter {
            adapter: "engram-ingest-git".to_owned(),
            message: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(CoreError::Adapter {
            adapter: "engram-ingest-git".to_owned(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(output.stdout)
}

fn invalid<T>(reason: impl Into<String>) -> CoreResult<T> {
    Err(CoreError::InvalidRequest {
        reason: reason.into(),
    })
}
