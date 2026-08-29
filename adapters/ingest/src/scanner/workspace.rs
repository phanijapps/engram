//! Workspace phase — `.engram-workspace` marker detection and multi-repo
//! scanning into a shared scope. (B8 — workspace fusion, RFC-0008)

use std::path::{Path, PathBuf};

use engram_knowledge::{CoreError, CoreResult, KnowledgeGraphRepository, KnowledgeRepository};

use crate::{ScanOptions, ScanProgress, ScanSummary, scanner::scan_repository};

pub(crate) const WORKSPACE_MARKER: &str = ".engram-workspace";

/// Detects workspace children: if `root` contains a `.engram-workspace` marker,
/// returns child directories that are git repos. Returns `None` if no marker or
/// no child repos. (B8 — workspace fusion, RFC-0008)
pub fn detect_workspace(root: &Path) -> Option<Vec<PathBuf>> {
    if !root.join(WORKSPACE_MARKER).exists() {
        return None;
    }
    let children: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
        .map(|e| e.path())
        .filter(|p| p.join(".git").exists())
        .collect();
    if children.is_empty() {
        None
    } else {
        Some(children)
    }
}

/// Scans a workspace: detects child repos via `.engram-workspace` marker and
/// scans each into the shared repository with the shared workspace scope. (B8)
pub fn scan_workspace<R>(
    root: &Path,
    opts: &ScanOptions,
    repo: &R,
    progress: &(impl Fn(ScanProgress) + Send + Sync),
) -> CoreResult<ScanSummary>
where
    R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync,
{
    let children = detect_workspace(root).ok_or_else(|| CoreError::InvalidRequest {
        reason: format!("no {WORKSPACE_MARKER} marker at {}", root.display()),
    })?;
    let mut summary = ScanSummary::default();
    for child in &children {
        let repo_name = child.file_name().and_then(|n| n.to_str()).unwrap_or("repo");
        let child_opts = ScanOptions {
            source_name: format!("{}:{repo_name}", opts.source_name),
            ..opts.clone()
        };
        let (child_summary, _) = scan_repository(child, &child_opts, repo, progress)?;
        summary.scanned += child_summary.scanned;
        summary.ingested += child_summary.ingested;
        summary.unchanged += child_summary.unchanged;
        summary.skipped += child_summary.skipped;
        summary.entities += child_summary.entities;
        summary.relationships += child_summary.relationships;
        summary.errors += child_summary.errors;
    }
    Ok(summary)
}
