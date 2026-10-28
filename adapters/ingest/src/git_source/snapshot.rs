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
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let temp = attempt
            .directory
            .as_ref()
            .expect("owned snapshot directory");
        let mut preflight = Vec::new();
        let mut directories = std::collections::BTreeSet::new();
        // Create every zero-byte destination before writing any content. The destination filesystem
        // rejects case/normalization/short-name aliases and file/directory conflicts itself.
        for (_, file) in &inventory {
            let parent = file.path.parent().unwrap_or(Path::new(""));
            let dir = temp.ensure_subdirectory(parent).map_err(io_error)?;
            let out = dir
                .create_file(Path::new(file.path.file_name().expect("validated leaf")))
                .map_err(io_error)?;
            preflight.push(super::private_storage::file_identity(&out).map_err(io_error)?);
            let mut path = parent;
            while !path.as_os_str().is_empty() {
                directories.insert(path.to_path_buf());
                path = path.parent().unwrap_or(Path::new(""));
            }
        }
        let mut hash = Sha256::new();
        for ((path, file), expected) in inventory.iter().zip(preflight) {
            let mut options = cap_std::fs::OpenOptions::new();
            options.write(true);
            let mut out = temp.dir.open_with(&file.path, &options).map_err(io_error)?;
            let actual = out.metadata().map_err(io_error)?;
            if !actual.is_file()
                || actual.len() != 0
                || super::private_storage::file_identity(&out).map_err(io_error)? != expected
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
        ]
        .into_iter()
        .enumerate()
        {
            let probe = temp.path().join(format!("probe-{n}"));
            std::fs::create_dir(&probe).unwrap();
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(probe.join(first))
                .unwrap();
            let distinct = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(probe.join(second))
                .is_ok();
            let root = temp.path().join(format!("managed-{n}"));
            let result = retain_snapshot(
                &root,
                commit(),
                &[file(first, b"first"), file(second, b"second")],
            );
            if distinct {
                let retained = result.unwrap();
                assert_eq!(std::fs::read(retained.root.join(first)).unwrap(), b"first");
                assert_eq!(
                    std::fs::read(retained.root.join(second)).unwrap(),
                    b"second"
                );
            } else {
                assert!(result.is_err());
                assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
            }
        }
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
