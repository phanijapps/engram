//! Adapter-owned retained-source reader (corrective r2, finding 7).
//!
//! Serves EXACT committed bytes from the immutable retained snapshot the
//! acquisition stage wrote (`retain_snapshot`: `<storage>/<commit-oid>/<path>`
//! under a private 0700 root). Custody follows the established
//! capability-relative rules: the storage root is re-opened through
//! `private_directory` (owned-directory validation, links rejected), every
//! path component is opened capability-relatively and refused when it is a
//! link or not a directory, and the leaf must be a regular file. Read bytes
//! are verified against the DURABLE source descriptor's content digest
//! before the read is declared exact — hashing arbitrary replacement bytes
//! is not exactness. All failures surface as sanitized
//! `ExactSourceUnavailable` errors (no filesystem error chains).

use engram_knowledge::{
    CoreResult, ExactSourceView, RetainedPath, RetainedSourceReader, content_digest,
    exact_unavailable,
};
use std::path::PathBuf;

use super::private_storage::private_directory;

fn invalid_oid(oid: &str) -> bool {
    !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Capability-relative retained-snapshot reader over a private storage root.
pub struct RetainedSnapshotReader {
    storage: PathBuf,
}

impl RetainedSnapshotReader {
    /// Wraps the storage root the acquisition stage owns. The root is
    /// revalidated on EVERY read (owned private directory, no links) — a
    /// substituted or removed root fails closed.
    pub fn new(storage: impl Into<PathBuf>) -> Self {
        Self {
            storage: storage.into(),
        }
    }
}

impl RetainedSourceReader for RetainedSnapshotReader {
    fn read_span(
        &self,
        commit_oid: &str,
        path: &str,
        span: Option<std::ops::Range<usize>>,
        expected_digest: &str,
    ) -> CoreResult<ExactSourceView> {
        if invalid_oid(commit_oid) {
            return Err(exact_unavailable(
                "exact source requires a full object-format-matching commit ID",
            ));
        }
        let path = RetainedPath::new(path)?;
        // (a) Root custody: private-directory validation rejects links,
        // foreign ownership and missing storage with a sanitized error.
        let root = private_directory(&self.storage).map_err(|_| {
            exact_unavailable("retained source storage is unavailable or not owned")
        })?;
        // (b) Commit directory: must exist as a real directory under the root.
        let commit_dir = root
            .dir
            .open_dir(commit_oid)
            .map_err(|_| exact_unavailable("no retained snapshot for the indexed commit"))?;
        if root
            .dir
            .symlink_metadata(commit_oid)
            .map(|meta| meta.file_type().is_symlink())
            .unwrap_or(true)
        {
            return Err(exact_unavailable(
                "retained snapshot identity does not match the indexed commit",
            ));
        }
        // (c) Capability-relative descent through every path component,
        // refusing links and non-directories (ancestor substitution).
        let mut current = commit_dir;
        let components: Vec<&str> = path.as_str().split('/').collect();
        for component in &components[..components.len().saturating_sub(1)] {
            if super::safety::validate_retained_component(component).is_err() {
                return Err(exact_unavailable("retained path is not canonical"));
            }
            let metadata = current
                .symlink_metadata(component)
                .map_err(|_| exact_unavailable("retained source is unavailable"))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(exact_unavailable(
                    "retained source ancestry was substituted or is unavailable",
                ));
            }
            current = current
                .open_dir(component)
                .map_err(|_| exact_unavailable("retained source is unavailable"))?;
        }
        // (d) Leaf: regular file, never a link (leaf substitution).
        let leaf = components.last().copied().unwrap_or_default();
        if super::safety::validate_retained_component(leaf).is_err() {
            return Err(exact_unavailable("retained path is not canonical"));
        }
        let metadata = current
            .symlink_metadata(leaf)
            .map_err(|_| exact_unavailable("no retained blob for the indexed path"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(exact_unavailable(
                "retained source blob was substituted or is unavailable",
            ));
        }
        let mut bytes = Vec::new();
        {
            let mut file = current
                .open(leaf)
                .map_err(|_| exact_unavailable("no retained blob for the indexed path"))?;
            std::io::Read::read_to_end(&mut file, &mut bytes)
                .map_err(|_| exact_unavailable("no retained blob for the indexed path"))?;
        }
        // (e) Integrity: the durable descriptor's digest must match the read
        // bytes; a mismatch means tampering — never serve it as exact. The
        // scanner stamps digests with a `sha256:` prefix; normalize it away
        // before comparing (the digest VALUE is the identity, not its tag).
        let expected_digest = expected_digest.trim_start_matches("sha256:");
        let full_digest = content_digest(&bytes);
        if full_digest != expected_digest {
            return Err(exact_unavailable(
                "retained blob content does not match the durable indexed-source digest",
            ));
        }
        let (served, span) = match span {
            None => (bytes.clone(), None),
            Some(range) => {
                let (start, end) = (range.start, range.end);
                if start > end || end > bytes.len() {
                    return Err(engram_knowledge::CoreError::InvalidRequest {
                        reason: "requested span is outside the retained blob bounds".into(),
                    });
                }
                (bytes[start..end].to_vec(), Some((start, end)))
            }
        };
        let served_digest = content_digest(&served);
        Ok(ExactSourceView {
            served_digest,
            full_content_digest: full_digest,
            bytes: served,
            commit_oid: commit_oid.to_owned(),
            path: path.as_str().to_owned(),
            span,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_source::{CommittedFile, ResolvedCommit, retain_snapshot};

    fn commit(oid: &str) -> ResolvedCommit {
        ResolvedCommit {
            branch_ref: "refs/heads/main".to_owned(),
            oid: oid.to_owned(),
            object_format: "sha1".to_owned(),
        }
    }

    /// Custody + integrity on a REAL retained snapshot: exact bytes serve;
    /// tampered bytes, symlink substitution and wrong digests deny sanitized.
    #[test]
    fn retained_reads_validate_custody_and_digest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let storage = dir.path().join("retained");
        let bytes = b"def alpha():\r\n    return 1\r\n".to_vec();
        let files = vec![CommittedFile {
            path: "src/mod.py".into(),
            oid: "b".repeat(40),
            mode: "100644".to_owned(),
            bytes: bytes.clone(),
        }];
        let snapshot = retain_snapshot(&storage, commit(&"c".repeat(40)), &files).expect("retain");
        let reader = RetainedSnapshotReader::new(storage.clone());
        let digest = crate::hash::content_hash(&bytes);

        // Exact read with the correct digest serves byte-exact content.
        let view = reader
            .read_span(&snapshot.commit.oid, "src/mod.py", None, &digest)
            .expect("exact read");
        assert_eq!(view.bytes, bytes);

        // Wrong digest (tampered-after-retention scenario) denies.
        let tampered = reader.read_span(&snapshot.commit.oid, "src/mod.py", None, &"0".repeat(64));
        assert!(
            matches!(&tampered, Err(engram_knowledge::CoreError::InvalidRequest { reason })
                if reason.contains("does not match the durable indexed-source digest")),
            "tampered bytes must never serve as exact: {tampered:?}"
        );

        // Leaf substitution with a symlink denies sanitized.
        let leaf = storage
            .join(&snapshot.commit.oid)
            .join("src")
            .join("mod.py");
        let impostor = dir.path().join("impostor.py");
        std::fs::write(&impostor, "IMPOSTER").expect("write impostor");
        std::fs::remove_file(&leaf).expect("remove leaf");
        std::os::unix::fs::symlink(&impostor, &leaf).expect("symlink leaf");
        let substituted = reader.read_span(&snapshot.commit.oid, "src/mod.py", None, &digest);
        assert!(
            matches!(&substituted, Err(engram_knowledge::CoreError::InvalidRequest { reason })
                if reason.contains("ExactSourceUnavailable")),
            "symlink substitution must deny sanitized: {substituted:?}"
        );

        // Ancestor substitution with a symlink denies sanitized.
        let ancestor_dir = storage.join(&snapshot.commit.oid).join("src");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).expect("mkdir outside");
        std::fs::write(outside.join("mod.py"), &bytes).expect("write outside copy");
        std::fs::remove_dir_all(&ancestor_dir).expect("remove ancestor dir");
        std::os::unix::fs::symlink(&outside, &ancestor_dir).expect("symlink ancestor");
        let ancestor_sub = reader.read_span(&snapshot.commit.oid, "src/mod.py", None, &digest);
        assert!(
            matches!(&ancestor_sub, Err(engram_knowledge::CoreError::InvalidRequest { reason })
                if reason.contains("ExactSourceUnavailable")),
            "ancestor substitution must deny sanitized: {ancestor_sub:?}"
        );

        // Missing storage denies sanitized (no filesystem error chains).
        let missing = RetainedSnapshotReader::new(dir.path().join("never-created"));
        let gone = missing.read_span(&snapshot.commit.oid, "src/mod.py", None, &digest);
        assert!(
            matches!(&gone, Err(engram_knowledge::CoreError::InvalidRequest { reason })
                if reason.contains("ExactSourceUnavailable")),
            "missing storage must deny sanitized: {gone:?}"
        );
    }
}
