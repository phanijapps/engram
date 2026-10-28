use super::{CommittedFile, ResolvedCommit, validate_branch_ref};
use engram_knowledge::{CoreError, CoreResult};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
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
struct PendingSnapshot(PathBuf);
impl Drop for PendingSnapshot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
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
pub(crate) fn private_directory(path: &Path) -> CoreResult<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(io_error)?;
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("snapshot storage must be a real private directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(invalid(
                "snapshot storage must be private to the current owner",
            ));
        }
    }
    Ok(())
}
fn sync_directory(path: &Path) -> CoreResult<()> {
    OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(io_error)?
        .sync_all()
        .map_err(io_error)
}

/// Retain exact bytes in a private, atomically published tree without consulting a checkout.
pub fn retain_snapshot(
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
    // Validate the entire inventory before creating anything on disk.
    let mut names = std::collections::HashSet::new();
    for file in files {
        if file.path.as_os_str().is_empty()
            || file
                .path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || !names.insert(file.path.clone())
            || !matches!(file.mode.as_str(), "100644" | "100755")
            || !valid_oid(&file.oid, &commit.object_format)
        {
            return Err(invalid(
                "snapshot inventory contains an invalid path, mode or object ID",
            ));
        }
    }
    private_directory(storage)?;
    let final_root = storage.join(&commit.oid);
    if final_root.exists() {
        return Err(CoreError::Conflict {
            reason: "snapshot for this commit already exists".into(),
        });
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temp = storage.join(format!(
        ".{}-{}-{}",
        commit.oid,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    // create_dir refuses an existing attempt rather than following a symlink.
    fs::create_dir(&temp).map_err(io_error)?;
    let pending = PendingSnapshot(temp.clone());
    let mut h = Sha256::new();
    let mut directories = std::collections::BTreeSet::new();
    directories.insert(temp.clone());
    for file in files {
        let path = file
            .path
            .to_str()
            .ok_or_else(|| invalid("snapshot path is not UTF-8"))?;
        h.update((path.len() as u64).to_le_bytes());
        h.update(path.as_bytes());
        h.update(file.mode.as_bytes());
        h.update((file.bytes.len() as u64).to_le_bytes());
        h.update(&file.bytes);
        let dest = temp.join(&file.path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
            let mut current = Some(parent);
            while let Some(dir) = current.filter(|dir| dir.starts_with(&temp)) {
                directories.insert(dir.to_path_buf());
                current = dir.parent();
            }
        }
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .map_err(io_error)?;
        out.write_all(&file.bytes).map_err(io_error)?;
        let mut perms = out.metadata().map_err(io_error)?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(&dest, perms).map_err(io_error)?;
        out.sync_all().map_err(io_error)?;
    }
    for dir in directories.iter().rev() {
        sync_directory(dir)?;
    }
    let digest = h
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    fs::rename(&temp, &final_root).map_err(io_error)?;
    sync_directory(storage)?;
    drop(pending);
    Ok(RetainedSnapshot {
        root: final_root,
        digest,
        commit,
    })
}
fn io_error(_: std::io::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-ingest-snapshot".into(),
        message: "managed source snapshot operation failed".into(),
    }
}
