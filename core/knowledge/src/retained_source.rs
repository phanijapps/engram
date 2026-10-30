//! Neutral port for reading retained committed source bytes with custody and
//! integrity owned by an ADAPTER (corrective r2, finding 7).
//!
//! Exact-source reads answer from the immutable retained snapshot the
//! acquisition stage wrote. The FILESYSTEM/CUSTODY mechanics (private
//! capability-relative storage, link/ancestor substitution rejection,
//! regular-file identity) live in the ingestion adapter behind this port —
//! neutral core never reads the filesystem for code sources. Every read is
//! validated against the durable indexed-source descriptor's content digest
//! before the bytes are declared exact: hashing arbitrary replacement bytes
//! is not exactness.

use engram_runtime::{CoreError, CoreResult};

/// A validated, relative, canonical retained-snapshot path (`a/b/c.rs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedPath(String);

impl RetainedPath {
    /// Validates and canonicalizes a snapshot-relative path: no empty
    /// components, no `.`/`..`, no lock-file tails — the same canonical shape
    /// the retention stage enforced at write time.
    pub fn new(path: &str) -> CoreResult<Self> {
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

/// One exact-source read result: byte-exact content, content digests, the
/// immutable revision it came from and the byte span served.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactSourceView {
    /// The exact retained bytes (possibly span-sliced, always from the
    /// indexed commit's blob, always digest-validated against the durable
    /// descriptor).
    pub bytes: Vec<u8>,
    /// SHA-256 over the FULL retained file bytes (identity of the revision's
    /// blob, independent of any span slice or output cap).
    pub full_content_digest: String,
    /// SHA-256 over the served slice (`== full_content_digest` when no span
    /// and no cap applied).
    pub served_digest: String,
    /// The commit the snapshot was retained for.
    pub commit_oid: String,
    /// The snapshot-relative path.
    pub path: String,
    /// The byte span served (`None` = whole file).
    pub span: Option<(usize, usize)>,
}

/// Adapter-owned retained-source reader (finding 7).
///
/// Implementations MUST:
/// - open the retained storage through established private/capability-
///   relative custody (owned directory validation; no symlink traversal; no
///   reads outside the retained root),
/// - reject leaf or ancestor substitution, tampering and missing storage
///   with SANITIZED `ExactSourceUnavailable` errors (no raw filesystem error
///   chains in agent diagnostics),
/// - verify the read bytes against `expected_digest` (the durable
///   descriptor's content digest) BEFORE returning: mismatch is tampering,
///   not exactness.
#[allow(clippy::double_must_use)]
// async_trait-free by design: adapter custody reads are sync-local.
pub trait RetainedSourceReader: Send + Sync {
    /// Reads the exact retained bytes of `path` at `commit_oid`, optionally
    /// restricted to a byte span `[start, end)` of the same blob. The
    /// `expected_digest` is the durable descriptor's FULL-file digest.
    fn read_span(
        &self,
        commit_oid: &str,
        path: &str,
        span: Option<std::ops::Range<usize>>,
        expected_digest: &str,
    ) -> CoreResult<ExactSourceView>;
}

/// SHA-256 content digest (hex) — shared so adapters and composition agree.
pub fn content_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.to_owned(),
    }
}

/// Sanitized typed unavailability (no filesystem error chains).
pub fn exact_unavailable(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: format!("ExactSourceUnavailable: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_canonical_paths() {
        assert!(RetainedPath::new("../escape.rs").is_err());
        assert!(RetainedPath::new("/abs.rs").is_err());
        assert!(RetainedPath::new("a//b.rs").is_err());
        assert!(RetainedPath::new("a/b.lock").is_err());
        assert!(RetainedPath::new("src/lib.rs").is_ok());
    }
}
