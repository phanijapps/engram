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
use std::io::Read;
use std::path::{Path, PathBuf};

use super::private_storage::{file_identity, named_identity, private_directory};

/// Retained snapshots originate from the acquisition reader's bounded file
/// policy.  Reapply that ceiling before allocating an exact-source buffer so
/// a substituted FIFO or oversized regular file cannot turn a read into an
/// unbounded allocation or block.
const MAX_RETAINED_READ_BYTES: u64 = 2 * 1024 * 1024;

fn open_regular_nonfollow(
    parent: &cap_std::fs::Dir,
    leaf: &str,
) -> std::io::Result<cap_std::fs::File> {
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        // O_NOFOLLOW rejects a symlink leaf; O_NONBLOCK guarantees a
        // substituted FIFO cannot BLOCK the open (a FIFO opened read-only
        // with no writer would hang). The opened handle's type is verified
        // afterwards, and O_NONBLOCK is harmless on regular files.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
        options.follow(FollowSymlinks::No);
    }
    parent.open_with(Path::new(leaf), &options)
}

fn invalid_oid(oid: &str) -> bool {
    !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Capability-relative retained-snapshot reader over a private storage root.
pub struct RetainedSnapshotReader {
    storage: PathBuf,
    /// Corrective r5 (A7): deterministic check→open observation seam — TEST
    /// ONLY (never compiled into production builds). Invoked exactly between
    /// a boundary's check (symlink_metadata + identity capture) and its
    /// corresponding open, so tests can substitute a different object with
    /// the SAME digest deterministically and prove the identity/type barrier
    /// fired — replacing the former probabilistic swapper-thread race.
    #[cfg(test)]
    check_open_hook: Option<std::sync::Arc<dyn Fn(CheckOpenBoundary) + Send + Sync>>,
}

/// The three check→open boundaries of [`RetainedSnapshotReader::read_span`]
/// (test instrumentation; see the `check_open_hook` field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckOpenBoundary {
    /// Between the commit directory’s check and its `open_dir`.
    CommitDirectory,
    /// Between an intermediate ancestor component’s check and its `open_dir`.
    Ancestor,
    /// Between the leaf’s check and its nofollow/nonblocking open.
    Leaf,
}

impl RetainedSnapshotReader {
    /// Wraps the storage root the acquisition stage owns. The root is
    /// revalidated on EVERY read (owned private directory, no links) — a
    /// substituted or removed root fails closed.
    pub fn new(storage: impl Into<PathBuf>) -> Self {
        Self {
            storage: storage.into(),
            #[cfg(test)]
            check_open_hook: None,
        }
    }

    /// Installs the test-only check→open observation seam.
    #[cfg(test)]
    pub(crate) fn set_check_open_hook(
        &mut self,
        hook: std::sync::Arc<dyn Fn(CheckOpenBoundary) + Send + Sync>,
    ) {
        self.check_open_hook = Some(hook);
    }

    /// Fires the test-only seam at `boundary` (no-op in production builds:
    /// the body, and the hook field, exist only under `cfg(test)`).
    #[cfg_attr(not(test), allow(unused_variables))]
    fn fire_check_open_hook(&self, boundary: CheckOpenBoundary) {
        #[cfg(test)]
        if let Some(hook) = &self.check_open_hook {
            hook(boundary);
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
        // (b) Commit directory: must exist as a real directory under the root,
        // and the OPENED handle must be the checked object — an identity
        // substitution between check and open denies even when the
        // replacement carries the same name.
        if root
            .dir
            .symlink_metadata(commit_oid)
            .map(|meta| meta.file_type().is_symlink() || !meta.is_dir())
            .unwrap_or(true)
        {
            return Err(exact_unavailable(
                "retained snapshot identity does not match the indexed commit",
            ));
        }
        let checked_commit =
            super::private_storage::named_identity(&root.dir, Path::new(commit_oid))
                .map_err(|_| exact_unavailable("no retained snapshot for the indexed commit"))?;
        self.fire_check_open_hook(CheckOpenBoundary::CommitDirectory);
        let commit_dir = root
            .dir
            .open_dir(commit_oid)
            .map_err(|_| exact_unavailable("no retained snapshot for the indexed commit"))?;
        if super::private_storage::directory_identity_of(&commit_dir)
            .map_err(|_| exact_unavailable("no retained snapshot for the indexed commit"))?
            != checked_commit
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
            // Bind the checked entry to the OPENED directory handle: a
            // substitution between check and open changes the opened
            // directory's identity and is denied (finding 7 race).
            let checked_ancestor =
                super::private_storage::named_identity(&current, Path::new(component))
                    .map_err(|_| exact_unavailable("retained source is unavailable"))?;
            self.fire_check_open_hook(CheckOpenBoundary::Ancestor);
            let opened = current
                .open_dir(component)
                .map_err(|_| exact_unavailable("retained source is unavailable"))?;
            if super::private_storage::directory_identity_of(&opened)
                .map_err(|_| exact_unavailable("retained source is unavailable"))?
                != checked_ancestor
            {
                return Err(exact_unavailable(
                    "retained source ancestry was substituted or is unavailable",
                ));
            }
            current = opened;
        }
        // (d) Leaf: regular file, never a link (leaf substitution).
        let leaf = components.last().copied().unwrap_or_default();
        if super::safety::validate_retained_component(leaf).is_err() {
            return Err(exact_unavailable("retained path is not canonical"));
        }
        let metadata = current
            .symlink_metadata(leaf)
            .map_err(|_| exact_unavailable("no retained blob for the indexed path"))?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAX_RETAINED_READ_BYTES
        {
            return Err(exact_unavailable(
                "retained source blob was substituted or is unavailable",
            ));
        }
        // Bind the checked directory entry to the opened no-follow handle.
        // A rename or link substitution between inspection and open changes
        // the handle identity and is denied even when replacement bytes hash
        // to the same digest.
        let checked_identity = named_identity(&current, Path::new(leaf)).map_err(|_| {
            exact_unavailable("retained source blob was substituted or is unavailable")
        })?;
        self.fire_check_open_hook(CheckOpenBoundary::Leaf);
        let mut file = open_regular_nonfollow(&current, leaf)
            .map_err(|_| exact_unavailable("no retained blob for the indexed path"))?;
        let opened_metadata = file
            .metadata()
            .map_err(|_| exact_unavailable("no retained blob for the indexed path"))?;
        if !opened_metadata.is_file()
            || opened_metadata.file_type().is_symlink()
            || opened_metadata.len() > MAX_RETAINED_READ_BYTES
            || file_identity(&file).map_err(|_| {
                exact_unavailable("retained source blob was substituted or is unavailable")
            })? != checked_identity
        {
            return Err(exact_unavailable(
                "retained source blob was substituted or is unavailable",
            ));
        }
        let size = usize::try_from(opened_metadata.len())
            .map_err(|_| exact_unavailable("retained source blob exceeds the read bound"))?;
        let mut bytes = Vec::with_capacity(size);
        file.by_ref()
            .take(MAX_RETAINED_READ_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| exact_unavailable("no retained blob for the indexed path"))?;
        if bytes.len() != size || bytes.len() as u64 > MAX_RETAINED_READ_BYTES {
            return Err(exact_unavailable(
                "retained source blob changed during the bounded read",
            ));
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
    #[cfg(unix)]
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

    /// Corrective r3 (finding 7 races): deterministic substitution barriers
    /// at the check→open boundaries. A SAME-DIGEST symlink impostor (identical
    /// bytes, so the digest alone cannot distinguish it) and a substituted
    /// FIFO (which would block a naive blocking open) must both deny
    /// sanitized and promptly — no hang, no replacement bytes served.
    #[cfg(unix)]
    #[test]
    fn same_digest_and_fifo_substitutions_deny_without_hanging() {
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
        let leaf = storage
            .join(&snapshot.commit.oid)
            .join("src")
            .join("mod.py");

        // Same-digest symlink impostor: the impostor file holds IDENTICAL
        // bytes (digest matches), so only link custody can deny it.
        let impostor = dir.path().join("same-digest-impostor.py");
        std::fs::write(&impostor, &bytes).expect("write identical impostor");
        std::fs::remove_file(&leaf).expect("remove leaf");
        std::os::unix::fs::symlink(&impostor, &leaf).expect("symlink identical impostor");
        let started = std::time::Instant::now();
        let same_digest = reader.read_span(&snapshot.commit.oid, "src/mod.py", None, &digest);
        assert!(
            matches!(&same_digest, Err(engram_knowledge::CoreError::InvalidRequest { reason })
                if reason.contains("ExactSourceUnavailable")),
            "a same-digest symlink substitution must deny: {same_digest:?}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "the denial must be prompt, not a hang"
        );

        // FIFO substitution: a naive blocking open would hang forever; the
        // nonblocking nofollow open must fail fast and sanitized. The FIFO
        // CREATION is asserted (canonical 1: a missing FIFO would silently
        // weaken this case to a plain not-found denial).
        std::fs::remove_file(&leaf).expect("remove symlink");
        nix_compat_fifo(&leaf).expect("FIFO creation must succeed");
        use std::os::unix::fs::FileTypeExt as _;
        assert!(
            std::fs::symlink_metadata(&leaf)
                .map(|meta| meta.file_type().is_fifo())
                .unwrap_or(false),
            "fixture sanity: the substituted leaf IS a FIFO"
        );
        let started_fifo = std::time::Instant::now();
        let fifo_result = reader.read_span(&snapshot.commit.oid, "src/mod.py", None, &digest);
        assert!(
            matches!(&fifo_result, Err(engram_knowledge::CoreError::InvalidRequest { reason })
                if reason.contains("ExactSourceUnavailable")),
            "a FIFO substitution must deny sanitized: {fifo_result:?}"
        );
        assert!(
            started_fifo.elapsed() < std::time::Duration::from_secs(10),
            "the FIFO denial must be prompt, not a blocking open"
        );
    }

    /// Corrective r5 (A7): the deterministic check→open seam suite REPLACES
    /// the former probabilistic swapper-thread race. Substitutions now land
    /// EXACTLY between each boundary's check and its corresponding open
    /// (commit directory, intermediate ancestor, leaf), and each denial is
    /// proven prompt through an EXTERNAL timeout boundary — a worker thread
    /// performs the read while the test waits on a channel with
    /// `recv_timeout`, so a blocking open FAILS the test at the timeout
    /// instead of hanging past it and only then evaluating an elapsed-time
    /// assertion.
    #[cfg(unix)]
    mod check_open_seams {
        use super::*;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        /// Runs one read on a worker thread bounded by an external timeout.
        /// `Ok(result)` — the read completed within the bound and the worker
        /// was JOINED (no leaked workers); the panic path is the deterministic
        /// no-hang proof: a blocking read fails the test at the timeout
        /// boundary itself, never via a post-hoc elapsed assertion.
        fn read_with_external_timeout(
            storage: std::path::PathBuf,
            commit_oid: &str,
            digest: &str,
            hook: Option<std::sync::Arc<dyn Fn(CheckOpenBoundary) + Send + Sync>>,
        ) -> CoreResult<ExactSourceView> {
            let (tx, rx) = std::sync::mpsc::channel::<()>();
            let commit_oid = commit_oid.to_owned();
            let digest = digest.to_owned();
            let worker = std::thread::spawn(move || {
                let mut reader = RetainedSnapshotReader::new(storage);
                if let Some(hook) = hook {
                    reader.set_check_open_hook(hook);
                }
                let result = reader.read_span(&commit_oid, "src/mod.py", None, &digest);
                let _ = tx.send(());
                result
            });
            match rx.recv_timeout(std::time::Duration::from_secs(30)) {
                Ok(()) => worker.join().expect("reader worker joined cleanly"),
                Err(_) => panic!(
                    "read_span blocked past the external timeout boundary — the                      nofollow/nonblocking open barriers must never hang"
                ),
            }
        }

        fn fixture(dir: &std::path::Path) -> (std::path::PathBuf, String, String, Vec<u8>) {
            let storage = dir.join("retained");
            let bytes = b"def alpha():\r\n    return 1\r\n".to_vec();
            let files = vec![CommittedFile {
                path: "src/mod.py".into(),
                oid: "b".repeat(40),
                mode: "100644".to_owned(),
                bytes: bytes.clone(),
            }];
            let snapshot = retain_snapshot(&storage, commit(&"c".repeat(40)), &files)
                .expect("retain snapshot");
            let digest = crate::hash::content_hash(&bytes);
            (storage, snapshot.commit.oid, digest, bytes)
        }

        fn hook_fired() -> (
            std::sync::Arc<dyn Fn(CheckOpenBoundary) + Send + Sync>,
            Arc<AtomicUsize>,
        ) {
            let fired = Arc::new(AtomicUsize::new(0));
            let counter = fired.clone();
            let hook: std::sync::Arc<dyn Fn(CheckOpenBoundary) + Send + Sync> =
                Arc::new(move |_boundary| {
                    counter.fetch_add(1, Ordering::SeqCst);
                });
            (hook, fired)
        }

        /// Control: an installed seam that only OBSERVES changes nothing —
        /// the read serves the exact digest-validated bytes and every
        /// boundary (commit directory, ancestor, leaf) fires exactly once.
        #[test]
        fn observing_seam_serves_exact_bytes_and_fires_every_boundary() {
            let dir = tempfile::tempdir().expect("tempdir");
            let (storage, oid, digest, bytes) = fixture(dir.path());
            let (hook, fired) = hook_fired();
            let view = read_with_external_timeout(storage, &oid, &digest, Some(hook))
                .expect("observing seam does not perturb the read");
            assert_eq!(view.bytes, bytes, "exact digest-validated bytes serve");
            assert_eq!(
                fired.load(Ordering::SeqCst),
                3,
                "commit-dir + ancestor + leaf seams all fired exactly once"
            );
        }

        /// SAME-DIGEST DIFFERENT-IDENTITY swaps land between the check and
        /// the open at EACH boundary: the replacement carries byte-identical
        /// content (the digest alone cannot distinguish it), so only the
        /// handle-identity barrier can deny — and it does, sanitized, at
        /// every boundary, promptly.
        #[test]
        fn same_digest_identity_swaps_deny_at_every_check_open_boundary() {
            for boundary in [
                CheckOpenBoundary::CommitDirectory,
                CheckOpenBoundary::Ancestor,
                CheckOpenBoundary::Leaf,
            ] {
                let dir = tempfile::tempdir().expect("tempdir");
                let (storage, oid, digest, bytes) = fixture(dir.path());
                let fired = Arc::new(AtomicUsize::new(0));
                let counter = fired.clone();
                let storage_for_hook = storage.clone();
                let hook: std::sync::Arc<dyn Fn(CheckOpenBoundary) + Send + Sync> =
                    Arc::new(move |fired_at| {
                        if !matches!(fired_at, boundary) {
                            return;
                        }
                        counter.fetch_add(1, Ordering::SeqCst);
                        let commit_dir = storage_for_hook.join(&oid_for(&storage_for_hook));
                        match fired_at {
                            CheckOpenBoundary::CommitDirectory => {
                                // Different-inode directory with IDENTICAL
                                // inner content swapped over the commit dir.
                                let moved = storage_for_hook.join("swapped-commit");
                                std::fs::rename(&commit_dir, &moved).expect("rename commit dir");
                                std::fs::create_dir_all(commit_dir.join("src"))
                                    .expect("recreate commit dir");
                                std::fs::write(commit_dir.join("src/mod.py"), &bytes)
                                    .expect("recreate identical content");
                            }
                            CheckOpenBoundary::Ancestor => {
                                let ancestor = commit_dir.join("src");
                                let moved = commit_dir.join("swapped-src");
                                std::fs::rename(&ancestor, &moved).expect("rename ancestor");
                                std::fs::create_dir_all(&ancestor).expect("recreate ancestor");
                                std::fs::write(ancestor.join("mod.py"), &bytes)
                                    .expect("recreate identical content");
                            }
                            CheckOpenBoundary::Leaf => {
                                let leaf = commit_dir.join("src/mod.py");
                                let fresh = commit_dir.join("src/.fresh-impostor");
                                std::fs::write(&fresh, &bytes).expect("write fresh impostor");
                                std::fs::rename(&fresh, &leaf).expect("swap leaf in place");
                            }
                        }
                    });
                // `oid` must be captured for the helper; recompute from storage.
                let result = read_with_external_timeout(storage.clone(), &oid, &digest, Some(hook));
                assert!(
                    matches!(&result, Err(engram_knowledge::CoreError::InvalidRequest { reason })
                        if reason.contains("ExactSourceUnavailable")),
                    "a same-digest different-identity swap at {boundary:?} must deny \
                     sanitized: {result:?}"
                );
                assert_eq!(
                    fired.load(Ordering::SeqCst),
                    1,
                    "the intended barrier fired exactly once ({boundary:?})"
                );
            }
        }

        fn oid_for(storage: &std::path::Path) -> String {
            // The single retained commit directory under the storage root.
            let entry = std::fs::read_dir(storage)
                .expect("read storage root")
                .next()
                .expect("one commit dir")
                .expect("entry");
            entry.file_name().to_string_lossy().into_owned()
        }

        /// TYPE substitutions at the leaf seam: a symlink, a FIFO (creation
        /// asserted, resulting file type asserted) and a nonregular directory
        /// in place of the file — every one denies sanitized and promptly
        /// under the external timeout boundary.
        #[test]
        fn leaf_type_substitutions_deny_without_hanging() {
            use std::os::unix::fs::FileTypeExt as _;

            enum LeafSwap {
                Symlink,
                Fifo,
                Directory,
            }
            for (swap, label) in [
                (LeafSwap::Symlink, "symlink"),
                (LeafSwap::Fifo, "FIFO"),
                (LeafSwap::Directory, "directory"),
            ] {
                let dir = tempfile::tempdir().expect("tempdir");
                let (storage, oid, digest, bytes) = fixture(dir.path());
                let fired = Arc::new(AtomicUsize::new(0));
                let counter = fired.clone();
                let storage_for_hook = storage.clone();
                let hook: std::sync::Arc<dyn Fn(CheckOpenBoundary) + Send + Sync> =
                    Arc::new(move |fired_at| {
                        if !matches!(fired_at, CheckOpenBoundary::Leaf) {
                            return;
                        }
                        counter.fetch_add(1, Ordering::SeqCst);
                        let leaf = storage_for_hook
                            .join(&oid_for(&storage_for_hook))
                            .join("src/mod.py");
                        match swap {
                            LeafSwap::Symlink => {
                                let target = storage_for_hook.join("leaf-symlink-target.py");
                                std::fs::write(&target, &bytes).expect("write target");
                                std::fs::remove_file(&leaf).expect("remove leaf");
                                std::os::unix::fs::symlink(&target, &leaf).expect("symlink leaf");
                            }
                            LeafSwap::Fifo => {
                                std::fs::remove_file(&leaf).expect("remove leaf");
                                nix_compat_fifo(&leaf).expect("FIFO creation must succeed");
                                assert!(
                                    std::fs::symlink_metadata(&leaf)
                                        .map(|meta| meta.file_type().is_fifo())
                                        .unwrap_or(false),
                                    "fixture sanity: the substituted leaf IS a FIFO"
                                );
                            }
                            LeafSwap::Directory => {
                                std::fs::remove_file(&leaf).expect("remove leaf");
                                std::fs::create_dir(&leaf).expect("directory in leaf place");
                            }
                        }
                    });
                let result = read_with_external_timeout(storage, &oid, &digest, Some(hook));
                assert!(
                    matches!(&result, Err(engram_knowledge::CoreError::InvalidRequest { reason })
                        if reason.contains("ExactSourceUnavailable")),
                    "a {label} substitution at the leaf must deny sanitized: {result:?}"
                );
                assert_eq!(
                    fired.load(Ordering::SeqCst),
                    1,
                    "the leaf barrier fired ({label})"
                );
            }
        }
    }

    /// Creates a FIFO at `path` without Unix-only std::os imports at module
    /// scope (the test module already isolates platform cfg; this keeps the
    /// creation explicit and the cleanup caller-owned).
    #[cfg(unix)]
    fn nix_compat_fifo(path: &std::path::Path) -> std::io::Result<()> {
        unsafe {
            let cpath = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
                .map_err(|e| std::io::Error::other(e))?;
            if libc::mkfifo(cpath.as_ptr(), 0o600) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }

    /// Portable positive custody coverage. Windows runs this against the real
    /// retained adapter without importing Unix-only permission or symlink APIs.
    #[cfg(windows)]
    #[test]
    fn retained_reads_exact_bytes_on_windows() {
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
        let view = RetainedSnapshotReader::new(storage)
            .read_span(
                &snapshot.commit.oid,
                "src/mod.py",
                None,
                &crate::hash::content_hash(&bytes),
            )
            .expect("exact retained source");
        assert_eq!(view.bytes, bytes);
    }
}
