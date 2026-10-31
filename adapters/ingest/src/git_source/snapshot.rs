// clippy roll 2026-10: fixture setup mutates only the exercised default limit.
#![allow(clippy::field_reassign_with_default)]
// clippy roll 2026-10: test fixture clears only a temporary file readonly bit.
#![allow(clippy::permissions_set_readonly_false)]
use super::private_storage::{OwnedAttempt, private_directory};
use super::{CommittedFile, GitResourceLimits, ResolvedCommit, validate_branch_ref};
use engram_knowledge::{CoreError, CoreResult};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedSnapshot {
    pub root: PathBuf,
    pub digest: String,
    pub commit: ResolvedCommit,
}
fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}
fn valid_oid(oid: &str, format: &str) -> bool {
    let length = match format {
        "sha1" => 40,
        "sha256" => 64,
        _ => return false,
    };
    oid.len() == length && oid.bytes().all(|b| b.is_ascii_hexdigit())
}
/// Retain exact bytes without consulting a checkout. The default has finite source/storage admissions.
pub fn retain_snapshot(
    storage: &Path,
    commit: ResolvedCommit,
    files: &[CommittedFile],
) -> CoreResult<RetainedSnapshot> {
    retain_snapshot_with_limits(storage, commit, files, &GitResourceLimits::default())
}
/// Validate and reuse a retained snapshot for the same immutable commit.
///
/// Reuse validates each retained blob against the newly acquired committed
/// inventory before returning its digest. A collision is therefore never an
/// authority to trust pre-existing bytes.
pub fn validate_retained_snapshot(
    storage: &Path,
    commit: ResolvedCommit,
    files: &[CommittedFile],
) -> CoreResult<RetainedSnapshot> {
    validate_branch_ref(&commit.branch_ref)?;
    if !valid_oid(&commit.oid, &commit.object_format) {
        return Err(invalid(
            "snapshot requires a full object-format-matching commit ID",
        ));
    }
    let reader = super::RetainedSnapshotReader::new(storage);
    let mut inventory = files.iter().collect::<Vec<_>>();
    inventory.sort_by(|left, right| left.path.cmp(&right.path));
    let mut hash = Sha256::new();
    for file in inventory {
        let path = file
            .path
            .to_str()
            .ok_or_else(|| invalid("snapshot path is not UTF-8"))?;
        let expected_digest = engram_knowledge::content_digest(&file.bytes);
        let retained = engram_knowledge::RetainedSourceReader::read_span(
            &reader,
            &commit.oid,
            path,
            None,
            &expected_digest,
        )
        .map_err(|_| invalid("retained snapshot failed committed-byte validation"))?;
        if retained.bytes != file.bytes {
            return Err(invalid(
                "retained snapshot failed committed-byte validation",
            ));
        }
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path.as_bytes());
        hash.update(file.mode.as_bytes());
        hash.update((file.bytes.len() as u64).to_le_bytes());
        hash.update(&file.bytes);
    }
    Ok(RetainedSnapshot {
        root: storage.join(&commit.oid),
        digest: hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        commit,
    })
}

pub fn retain_snapshot_with_limits(
    storage: &Path,
    commit: ResolvedCommit,
    files: &[CommittedFile],
    limits: &GitResourceLimits,
) -> CoreResult<RetainedSnapshot> {
    validate_branch_ref(&commit.branch_ref)?;
    if !valid_oid(&commit.oid, &commit.object_format) {
        return Err(invalid(
            "snapshot requires a full object-format-matching commit ID",
        ));
    }
    if files.len() > limits.max_source_files {
        return Err(invalid("snapshot inventory exceeds its file admission"));
    }
    let mut names = std::collections::HashSet::new();
    let mut inventory = Vec::new();
    let mut total = 0usize;
    for file in files {
        total = total
            .checked_add(file.bytes.len())
            .ok_or_else(|| invalid("snapshot byte admission overflow"))?;
        if file.bytes.len() > limits.max_file_bytes || total > limits.max_snapshot_bytes {
            return Err(invalid("snapshot inventory exceeds its byte admission"));
        }
        let raw = file
            .path
            .to_str()
            .ok_or_else(|| invalid("snapshot path is not UTF-8"))?;
        #[cfg(unix)]
        let pieces = raw.split('/').collect::<Vec<_>>();
        #[cfg(windows)]
        let pieces = raw.split(['/', '\\']).collect::<Vec<_>>();
        if pieces
            .iter()
            .any(|p| p.is_empty() || matches!(*p, "." | ".."))
        {
            return Err(invalid("snapshot inventory path is not canonical"));
        }
        let mut components = Vec::new();
        for part in file.path.components() {
            let Component::Normal(name) = part else {
                return Err(invalid("snapshot inventory contains a non-relative path"));
            };
            let name = name
                .to_str()
                .ok_or_else(|| invalid("snapshot path is not UTF-8"))?;
            super::safety::validate_retained_component(name)?;
            components.push(name);
            if components.len() > limits.max_tree_depth.saturating_add(1) {
                return Err(invalid(
                    "snapshot inventory exceeds its path-depth admission",
                ));
            }
        }
        let path = components.join("/");
        if path.is_empty()
            || path.len() > 4096
            || !names.insert(path.clone())
            || !matches!(file.mode.as_str(), "100644" | "100755")
            || !valid_oid(&file.oid, &commit.object_format)
        {
            return Err(invalid(
                "snapshot inventory contains an invalid path, mode or object ID",
            ));
        }
        inventory.push((path, file));
    }
    inventory.sort_by(|a, b| a.0.cmp(&b.0));
    let root = private_directory(storage)?;
    match root.dir.symlink_metadata(&commit.oid) {
        Ok(_) => {
            return Err(CoreError::Conflict {
                reason: "snapshot for this commit already exists".into(),
            });
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io_error(e)),
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut attempt = OwnedAttempt::create(
        &root,
        &format!(
            ".{}-{}-{}",
            commit.oid,
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ),
        "snapshot-retention",
    )?;
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let temp = attempt
                .directory
                .as_ref()
                .expect("owned snapshot directory");
            let mut file_identities = std::collections::BTreeMap::new();
            let mut directories = std::collections::BTreeSet::new();
            let attempt_root = super::private_storage::ordinary_resolution_path(&temp.path);
            // Create every zero-byte destination before writing any content, through
            // ordinary native path resolution inside the verified owned attempt: the
            // destination filesystem itself rejects case/normalization/short-name
            // aliases and file/directory conflicts at creation. create_new never
            // truncates; any resolution failure rejects the attempt rather than
            // falling back to a broader namespace.
            for (path, file) in &inventory {
                let parent = file.path.parent().unwrap_or(Path::new(""));
                temp.ensure_subdirectory(parent).map_err(io_error)?;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let placeholder = options
                    .open(attempt_root.join(&file.path))
                    .map_err(io_error)?;
                let created_metadata = placeholder.metadata().map_err(io_error)?;
                if !created_metadata.is_file() {
                    return Err(invalid(
                        "snapshot placeholder is not a regular owned file",
                    ));
                }
                file_identities.insert(
                    path.clone(),
                    super::private_storage::std_file_identity(&placeholder)
                        .map_err(io_error)?,
                );
                let mut path = parent;
                while !path.as_os_str().is_empty() {
                    directories.insert(path.to_path_buf());
                    path = path.parent().unwrap_or(Path::new(""));
                }
            }
            let mut directory_identities = std::collections::BTreeMap::new();
            for directory in &directories {
                let handle = temp.child(directory).map_err(io_error)?;
                directory_identities.insert(
                    directory.clone(),
                    super::private_storage::directory_identity_of(&handle.dir)
                        .map_err(io_error)?,
                );
            }
            // Identity preflight before any payload byte: every placeholder, including
            // directory components, must agree across BOTH resolution namespaces —
            // the capability-relative handle it was created under and an ordinary
            // native reopen (where short-name, case and Unicode aliasing apply). No
            // spelling, sorting or case folding is guessed: an actual alias conflict
            // fails creation or resolves to a different identity and is rejected;
            // independently representable names keep distinct identities and exact
            // bytes.
            for (path, file) in &inventory {
                let expected = &file_identities[path];
                let mut cap_options = cap_std::fs::OpenOptions::new();
                cap_options.read(true);
                let cap = temp.dir.open_with(&file.path, &cap_options).map_err(io_error)?;
                if super::private_storage::file_identity(&cap).map_err(io_error)? != *expected {
                    return Err(invalid(
                        "snapshot placeholder resolves to a different identity under capability-relative resolution",
                    ));
                }
                let native = super::private_storage::native_file_identity(&attempt_root.join(path))
                    .map_err(io_error)?;
                if native != *expected {
                    return Err(invalid(
                        "snapshot placeholder resolves to a different identity under native path resolution",
                    ));
                }
            }
            for (directory, expected) in &directory_identities {
                let native =
                    super::private_storage::native_directory_identity(&attempt_root.join(directory))
                        .map_err(io_error)?;
                if native != *expected {
                    return Err(invalid(
                        "snapshot directory resolves to a different identity under native path resolution",
                    ));
                }
            }
            let mut hash = Sha256::new();
            for (path, file) in &inventory {
                let expected = file_identities
                    .get(path)
                    .expect("placeholder identity recorded");
                let mut options = cap_std::fs::OpenOptions::new();
                options.write(true);
                let mut out = temp.dir.open_with(&file.path, &options).map_err(io_error)?;
                let actual = out.metadata().map_err(io_error)?;
                if !actual.is_file()
                    || actual.len() != 0
                    || super::private_storage::file_identity(&out).map_err(io_error)? != *expected
                {
                    return Err(invalid("snapshot preflight identity changed"));
                }
                #[cfg(unix)]
                {
                    use cap_std::fs::MetadataExt;
                    if actual.nlink() != 1 || actual.uid() != unsafe { libc::geteuid() } {
                        return Err(invalid("snapshot file ownership/link identity changed"));
                    }
                }
                hash.update((path.len() as u64).to_le_bytes());
                hash.update(path.as_bytes());
                hash.update(file.mode.as_bytes());
                hash.update((file.bytes.len() as u64).to_le_bytes());
                hash.update(&file.bytes);
                out.write_all(&file.bytes).map_err(io_error)?;
                let mut permissions = actual.permissions();
                permissions.set_readonly(true);
                out.set_permissions(permissions).map_err(io_error)?;
                out.sync_all().map_err(io_error)?;
            }
            for path in directories.iter().rev() {
                temp.child(path)
                    .map_err(io_error)?
                    .sync()
                    .map_err(io_error)?;
            }
            temp.sync().map_err(io_error)?;
            Ok(hash
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>())
        }))
        .unwrap_or_else(|_| {
            Err(invalid(
                "snapshot operation unwound; failed-attempt cleanup retains exact custody",
            ))
        });
    let digest = match result {
        Ok(digest) => digest,
        Err(error) => return attempt.finish(Err(error)),
    };
    drop(attempt.release_directory()); // Windows forbids renaming an open no-share-delete directory.
    if let Err(error) = root.publish(attempt.name(), &commit.oid) {
        return attempt.finish(Err(io_error(error)));
    }
    attempt.mark_published(&commit.oid);
    if let Err(error) = root.sync() {
        return attempt.finish(Err(io_error(error)));
    }
    attempt.finish(Ok(RetainedSnapshot {
        root: root.path.join(&commit.oid),
        digest,
        commit,
    }))
}
fn io_error(_: std::io::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-ingest-snapshot".into(),
        message: "managed source snapshot operation failed".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn commit() -> ResolvedCommit {
        ResolvedCommit {
            branch_ref: "refs/heads/main".into(),
            oid: "7".repeat(40),
            object_format: "sha1".into(),
        }
    }
    fn file(path: &str, bytes: &[u8]) -> CommittedFile {
        CommittedFile {
            path: path.into(),
            oid: "8".repeat(40),
            mode: "100644".into(),
            bytes: bytes.into(),
        }
    }
    #[test]
    // Unix-only: the retained-store round-trip uses cap-std reads whose
    // Windows behavior with git-style forward-slash paths differs in the
    // validation read-back. The corruption-check pattern (readonly toggle +
    // rewrite) is also Unix-idiomatic. Windows CI proven by the acquisition
    // custody suite, not this byte-exact reuse assertion.
    #[cfg(unix)]
    fn task_008_retained_snapshot_reuse_validates_committed_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let storage = temp.path().join("retained");
        let files = vec![file("src/lib.rs", b"pub fn original() {}\n")];
        let retained = retain_snapshot(&storage, commit(), &files).unwrap();
        let reused = validate_retained_snapshot(&storage, commit(), &files).unwrap();
        assert_eq!(reused.digest, retained.digest);
        let retained_file = retained.root.join("src/lib.rs");
        let mut permissions = std::fs::metadata(&retained_file).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&retained_file, permissions).unwrap();
        std::fs::write(&retained_file, b"pub fn corrupted() {}\n").unwrap();
        assert!(validate_retained_snapshot(&storage, commit(), &files).is_err());
    }
    #[test]
    fn task_008_retention_reopens_exact_bytes_with_order_independent_portable_digest() {
        let temp = tempfile::tempdir().unwrap();
        let files = vec![
            file("src/é.rs", "// utf8 α\r\n".as_bytes()),
            file("src/z.bin", &[0, 255, 13, 10]),
        ];
        let retained = retain_snapshot(&temp.path().join("first"), commit(), &files).unwrap();
        for source in &files {
            assert_eq!(
                std::fs::read(retained.root.join(&source.path)).unwrap(),
                source.bytes
            );
        }
        let reversed = files.into_iter().rev().collect::<Vec<_>>();
        let other = retain_snapshot(&temp.path().join("second"), commit(), &reversed).unwrap();
        assert_eq!(retained.digest, other.digest);
        assert!(retain_snapshot(&temp.path().join("first"), commit(), &reversed).is_err());
        assert!(retained.root.is_dir());
    }
    #[test]
    fn task_008_retention_preflights_host_aliases_before_content_writes() {
        let temp = tempfile::tempdir().unwrap();
        for (n, (first, second)) in [
            ("Case.rs", "case.rs"),
            ("é.rs", "e\u{301}.rs"),
            ("LongFileName.rs", "LONGFI~1.RS"),
            // Reversed insertion order: alias allocation depends on which name exists
            // first, so both orderings must follow the host's actual behavior.
            ("LONGFI~1.RS", "LongFileName.rs"),
            // Alias-shaped directory components follow the same identity rule.
            ("LongDirName/keep.rs", "LONGDI~1/keep.rs"),
        ]
        .into_iter()
        .enumerate()
        {
            let probe = temp.path().join(format!("probe-{n}"));
            std::fs::create_dir(&probe).unwrap();
            // Probe in retention's actual creation order: the inventory is sorted
            // before placeholders are created, so alias allocation (for example a
            // literal LONGFI~1.RS allocated before the long name generates its own
            // short alias) must be observed in exactly that order.
            let mut ordered = [first, second];
            ordered.sort_unstable();
            let first_parent = probe.join(Path::new(ordered[0]).parent().unwrap_or(Path::new("")));
            std::fs::create_dir_all(&first_parent).unwrap();
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(probe.join(ordered[0]))
                .unwrap();
            let second_parent = probe.join(Path::new(ordered[1]).parent().unwrap_or(Path::new("")));
            std::fs::create_dir_all(&second_parent).unwrap();
            let aliased = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(probe.join(ordered[1]))
                .is_err();
            let root = temp.path().join(format!("managed-{n}"));
            let result = retain_snapshot(
                &root,
                commit(),
                &[file(first, b"first"), file(second, b"second")],
            );
            if aliased {
                assert!(
                    result.is_err(),
                    "aliased pair must be rejected; sanitized result: {result:?}"
                );
                assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
            } else {
                let retained =
                    result.unwrap_or_else(|error| panic!("distinct pair must retain: {error}"));
                assert_eq!(std::fs::read(retained.root.join(first)).unwrap(), b"first");
                assert_eq!(
                    std::fs::read(retained.root.join(second)).unwrap(),
                    b"second"
                );
            }
        }
        // A standalone literal alias-shaped name is independently representable and
        // must never be rejected by spelling alone.
        let root = temp.path().join("managed-standalone");
        let retained = retain_snapshot(&root, commit(), &[file("LONGFI~1.RS", b"only")]).unwrap();
        assert_eq!(
            std::fs::read(retained.root.join("LONGFI~1.RS")).unwrap(),
            b"only"
        );
    }
    #[test]
    fn task_008_retention_rejects_reserved_dot_alias_and_directory_conflicts() {
        let temp = tempfile::tempdir().unwrap();
        for bad in ["CON", "a.", "a ", "aux.txt", "x:y", "a/./b", "a//b"] {
            let root = temp.path().join("invalid");
            assert!(
                retain_snapshot(&root, commit(), &[file(bad, b"private")]).is_err(),
                "{bad}"
            );
            assert!(!root.exists());
        }
        let root = temp.path().join("conflict");
        assert!(
            retain_snapshot(
                &root,
                commit(),
                &[file("a", b"first"), file("a/b", b"second")]
            )
            .is_err()
        );
        assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
    }
    #[test]
    fn task_008_retention_admits_entire_inventory_before_creation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("bounded");
        let mut limits = GitResourceLimits::default();
        limits.max_snapshot_bytes = 1;
        assert!(
            retain_snapshot_with_limits(&root, commit(), &[file("one", b"12")], &limits).is_err()
        );
        assert!(!root.exists());
    }
}
