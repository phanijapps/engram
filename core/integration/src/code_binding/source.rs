//! Retained committed source reads: exact bytes and same-OID spans from the
//! immutable snapshot store, with honest unavailability errors and NO checkout
//! fallback (task_017, REQ-006 / AC-008).
//!
//! Bound code fetches answer from the retained blob bytes the acquisition
//! stage wrote (`retain_snapshot`: `<root>/<commit-oid>/<path>`, exact bytes,
//! content digests, immutable). A missing snapshot is a typed error — the
//! reader never falls back to a working-tree read, so a checkout that has
//! changed or been deleted can never masquerade as the indexed revision.

use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

fn invalid(reason: &str) -> engram_runtime::CoreError {
    engram_runtime::CoreError::InvalidRequest {
        reason: reason.to_owned(),
    }
}

fn exact_unavailable(reason: &str) -> engram_runtime::CoreError {
    engram_runtime::CoreError::InvalidRequest {
        reason: format!("ExactSourceUnavailable: {reason}"),
    }
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A validated, relative, canonical retained-snapshot path (`a/b/c.rs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedPath(String);

impl RetainedPath {
    /// Validates and canonicalizes a snapshot-relative path: no empty
    /// components, no `.`/`..`, no separators that escape the snapshot — the
    /// same canonical shape the retention stage enforced at write time.
    pub fn new(path: &str) -> engram_runtime::CoreResult<Self> {
        if path.is_empty() || path.len() > 4096 {
            return Err(invalid("retained path is empty or too long"));
        }
        let mut components = Vec::new();
        for part in path.split('/') {
            if part.is_empty() || matches!(part, "." | "..") || part.ends_with(".lock") {
                return Err(invalid("retained path is not canonical"));
            }
            components.push(part.to_owned());
        }
        Ok(Self(components.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One exact-source read result: byte-exact content, its content digest, the
/// immutable revision it came from and the (optional) byte span served.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactSourceView {
    /// The exact retained bytes (possibly span-sliced, always from the
    /// indexed commit's blob).
    pub bytes: Vec<u8>,
    /// SHA-256 over the FULL retained file bytes (identity of the revision's
    /// blob, independent of any span slice).
    pub full_content_digest: String,
    /// SHA-256 over the served slice (`== full_content_digest` when no span).
    pub served_digest: String,
    /// The commit the snapshot was retained for.
    pub commit_oid: String,
    /// The snapshot-relative path.
    pub path: String,
    /// The byte span served (`None` = whole file).
    pub span: Option<(usize, usize)>,
}

/// Reads exact committed bytes from a retained-snapshot root.
///
/// The root is the storage directory the acquisition stage owns
/// (`retain_snapshot(storage, ..)` writes `<storage>/<oid>/<path>`); reads
/// resolve inside it with ordinary path validation only — no symlink
/// traversal, no absolute paths, and never a lookup outside the root.
pub struct RetainedCodeSource {
    root: PathBuf,
}

impl RetainedCodeSource {
    /// Wraps a retained-snapshot storage root. The root must already exist
    /// (an absent root yields `ExactSourceUnavailable` on every read, never a
    /// fallback).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The wrapped root (diagnostics).
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn snapshot_path(&self, oid: &str, path: &RetainedPath) -> engram_runtime::CoreResult<PathBuf> {
        if !valid_oid(oid) {
            return Err(invalid(
                "exact source requires a full object-format-matching commit ID",
            ));
        }
        let mut target = self.root.join(oid);
        for component in path.as_str().split('/') {
            target.push(component);
        }
        // Defense in depth: the constructed path stays inside the root and is
        // composed of normal components only (the root's own prefix is
        // excluded from the check).
        let suffix = target
            .strip_prefix(&self.root)
            .map_err(|_| invalid("retained path escapes the snapshot root"))?;
        for component in suffix.components() {
            if !matches!(component, Component::Normal(_)) {
                return Err(invalid("retained path escapes the snapshot root"));
            }
        }
        Ok(target)
    }

    /// Reads the exact retained bytes of `path` at `commit_oid`.
    /// Errors with `ExactSourceUnavailable` when the snapshot is absent —
    /// NEVER falls back to a checkout or working-tree read.
    pub fn read(
        &self,
        commit_oid: &str,
        path: &str,
    ) -> engram_runtime::CoreResult<ExactSourceView> {
        self.read_span(commit_oid, path, None)
    }

    /// [`Self::read`] restricted to a byte span `[start, end)` of the SAME
    /// indexed revision — spans are byte offsets into the retained blob, so a
    /// truncated or re-sliced checkout can never shift them.
    pub fn read_span(
        &self,
        commit_oid: &str,
        path: &str,
        span: Option<std::ops::Range<usize>>,
    ) -> engram_runtime::CoreResult<ExactSourceView> {
        let path = RetainedPath::new(path)?;
        let target = self.snapshot_path(commit_oid, &path)?;
        let bytes = std::fs::read(&target).map_err(|error| {
            exact_unavailable(&format!(
                "no retained snapshot for commit {commit_oid} path {}: {error}",
                path.as_str()
            ))
        })?;
        let full_digest = content_digest(&bytes);
        let (served, span) = match span {
            None => (bytes.clone(), None),
            Some(range) => {
                let start = range.start;
                let end = range.end;
                if start > end || end > bytes.len() {
                    return Err(invalid(
                        "requested span is outside the retained blob bounds",
                    ));
                }
                (bytes[start..end].to_vec(), Some((start, end)))
            }
        };
        let served_digest = content_digest(&served);
        Ok(ExactSourceView {
            bytes: served,
            full_content_digest: full_digest,
            served_digest,
            commit_oid: commit_oid.to_owned(),
            path: path.as_str().to_owned(),
            span,
        })
    }
}

/// SHA-256 content digest (hex).
pub fn content_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl crate::code_binding::CodeBindingCoordinator {
    /// Serves the EXACT committed bytes of `path` for a bound repository's
    /// published generation, from the retained snapshot — same-OID spans
    /// optional (task_017, REQ-006).
    ///
    /// The chain is durable end to end: binding → active generation →
    /// generation record (immutable commit OID) → retained blob. A missing
    /// snapshot, an unbound repository or an unpublished generation is a
    /// typed error; there is NO checkout or working-tree fallback anywhere
    /// in the chain.
    pub async fn exact_source(
        &self,
        scope: &engram_domain::Scope,
        repo_id: &str,
        generation_id: Option<&str>,
        path: &str,
        span: Option<std::ops::Range<usize>>,
        retained: &RetainedCodeSource,
    ) -> engram_runtime::CoreResult<ExactSourceView> {
        let repo = engram_domain::RepoId(repo_id.to_owned());
        let binding = self.store.get_binding(scope, &repo).await?.ok_or_else(|| {
            engram_runtime::CoreError::InvalidRequest {
                reason: "BindingRequired: no bound repository for exact source".into(),
            }
        })?;
        let generation_id = match generation_id {
            Some(id) => id.to_owned(),
            None => binding.active_generation.clone().ok_or_else(|| {
                engram_runtime::CoreError::InvalidRequest {
                    reason:
                        "ExactSourceUnavailable: the bound repository has no published generation"
                            .into(),
                }
            })?,
        };
        let generation = self
            .store
            .get_generation(scope, &repo, &generation_id)
            .await?
            .ok_or_else(|| engram_runtime::CoreError::InvalidRequest {
                reason: "ExactSourceUnavailable: the published generation record is missing".into(),
            })?;
        retained.read_span(&generation.commit_oid, path, span)
    }
}

impl crate::EngramProvider {
    /// Serves the EXACT committed bytes of `path` for a bound repository's
    /// published generation through the provider's retained-source handle
    /// (task_017). See [`CodeBindingCoordinator::exact_source`].
    pub async fn code_fetch_exact(
        &self,
        scope: &engram_domain::Scope,
        repo_id: &str,
        path: &str,
        span: Option<std::ops::Range<usize>>,
    ) -> engram_runtime::CoreResult<ExactSourceView> {
        let coordinator = self.code_bindings()?;
        let retained = self.retained_code_source()?;
        coordinator
            .exact_source(scope, repo_id, None, path, span, &retained)
            .await
    }

    /// [`Self::code_fetch_exact`] pinned to one indexed generation — the
    /// revision a specific chunk set was stamped with, which may be older
    /// than the active generation. Bytes still come from the retained
    /// snapshot of THAT commit (same-OID honesty), never from a checkout.
    pub async fn code_fetch_exact_at(
        &self,
        scope: &engram_domain::Scope,
        repo_id: &str,
        generation_id: &str,
        path: &str,
        span: Option<std::ops::Range<usize>>,
    ) -> engram_runtime::CoreResult<ExactSourceView> {
        let coordinator = self.code_bindings()?;
        let retained = self.retained_code_source()?;
        coordinator
            .exact_source(scope, repo_id, Some(generation_id), path, span, &retained)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_canonical_paths_and_oids() {
        assert!(RetainedPath::new("../escape.rs").is_err());
        assert!(RetainedPath::new("/abs.rs").is_err());
        assert!(RetainedPath::new("a//b.rs").is_err());
        assert!(RetainedPath::new("src/lib.rs").is_ok());
        let source = RetainedCodeSource::new("/nonexistent-snapshots");
        assert!(matches!(
            source.read("short", "src/lib.rs"),
            Err(engram_runtime::CoreError::InvalidRequest { .. })
        ));
    }
}
