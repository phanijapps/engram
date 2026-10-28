use super::{CommittedFile, ResolvedCommit};
use engram_knowledge::{CoreError, CoreResult};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedSnapshot {
    pub root: PathBuf,
    pub digest: String,
    pub commit: ResolvedCommit,
}
pub fn retain_snapshot(
    storage: &Path,
    commit: ResolvedCommit,
    files: &[CommittedFile],
) -> CoreResult<RetainedSnapshot> {
    fs::create_dir_all(storage).map_err(io_error)?;
    let final_root = storage.join(&commit.oid);
    if final_root.exists() {
        return Err(CoreError::Conflict {
            reason: "snapshot for this commit already exists".into(),
        });
    }
    let temp = storage.join(format!(".{}-{}", commit.oid, std::process::id()));
    fs::create_dir(&temp).map_err(io_error)?;
    let mut h = Sha256::new();
    for file in files {
        let path = &file.path;
        if path.is_absolute()
            || path.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            let _ = fs::remove_dir_all(&temp);
            return Err(CoreError::InvalidRequest {
                reason: "snapshot path escapes managed root".into(),
            });
        }
        h.update(path.to_string_lossy().as_bytes());
        h.update(&file.bytes);
        let dest = temp.join(path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(io_error)?
        }
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .map_err(io_error)?;
        out.write_all(&file.bytes).map_err(io_error)?;
        out.sync_all().map_err(io_error)?;
        let mut perms = out.metadata().map_err(io_error)?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(&dest, perms).map_err(io_error)?;
    }
    let digest = h
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    fs::rename(&temp, &final_root).map_err(io_error)?;
    if let Ok(dir) = OpenOptions::new().read(true).open(storage) {
        let _ = dir.sync_all();
    }
    Ok(RetainedSnapshot {
        root: final_root,
        digest,
        commit,
    })
}
fn io_error(e: std::io::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-ingest-snapshot".into(),
        message: e.to_string(),
    }
}
