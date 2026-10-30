//! Binding-keyed durable readiness manifests for attempt-owned knowledge writes.
use crate::{PreparedGeneration, content_hash, write_prepared_generation};
use engram_domain::CodeBinding;
use engram_knowledge::{
    CodeGenerationStaging, CoreError, CoreResult, KnowledgeGraphRepository, KnowledgeRepository,
    StageReceipt,
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_MANIFEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn invalid() -> CoreError {
    CoreError::InvalidRequest {
        reason: "generation manifest custody or identity is invalid".into(),
    }
}
fn io_error(_: std::io::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-generation-manifest".into(),
        message: "generation manifest operation failed".into(),
    }
}

/// Derive the manifest identity from authority and parser/filter inputs, never a checkout basename.
pub fn generation_manifest_key(
    binding: &CodeBinding,
    prepared: &PreparedGeneration,
    attempt_id: &str,
) -> CoreResult<String> {
    binding.validate().map_err(|_| invalid())?;
    prepared.generation.validate().map_err(|_| invalid())?;
    if attempt_id.is_empty()
        || binding.version != prepared.generation.binding_version
        || binding.scope != prepared.binding.scope
        || binding.repo_id != prepared.binding.repo_id
        || binding.branch_ref != prepared.binding.branch_ref
        || binding.version != prepared.binding.version
    {
        return Err(invalid());
    }
    let identity = serde_json::to_vec(&(
        &binding.scope,
        &binding.repo_id,
        &binding.branch_ref,
        binding.version,
        &prepared.generation,
        attempt_id,
    ))
    .map_err(|_| invalid())?;
    Ok(content_hash(identity).trim_start_matches("sha256:").into())
}
fn receipt(
    binding: &CodeBinding,
    prepared: &PreparedGeneration,
    attempt_id: &str,
) -> CoreResult<StageReceipt> {
    let sources = prepared
        .documents
        .iter()
        .map(|doc| &doc.source)
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&(
        generation_manifest_key(binding, prepared, attempt_id)?,
        &prepared.lineage,
        sources,
    ))
    .map_err(|_| invalid())?;
    Ok(StageReceipt {
        attempt_id: attempt_id.into(),
        generation_id: prepared.generation.id.clone(),
        lane: "knowledge".into(),
        digest: content_hash(bytes).trim_start_matches("sha256:").into(),
    })
}
struct TemporaryManifest(PathBuf);
impl Drop for TemporaryManifest {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Emit readiness only after all required store writes and the manifest's atomic fsync boundary.
/// The caller durably records the returned receipt before publication; this is not cross-store ACID.
pub async fn stage_generation_records<R>(
    root: &Path,
    binding: &CodeBinding,
    prepared: &PreparedGeneration,
    attempt_id: &str,
    store: &R,
) -> CoreResult<StageReceipt>
where
    R: KnowledgeRepository + KnowledgeGraphRepository + CodeGenerationStaging + ?Sized,
{
    stage_generation_records_with_options(root, binding, prepared, attempt_id, store, false, false)
        .await
}

/// Test seam for native durability and writer-exclusion proofs: the post-rename
/// directory sync can be forced to fail (modeling a crash between rename and the
/// durability boundary) and a writer can be paused while its temporary manifest is
/// published (modeling a paused or crashed exclusive owner). Production callers use
/// the plain wrapper.
#[doc(hidden)]
pub async fn stage_generation_records_with_options<R>(
    root: &Path,
    binding: &CodeBinding,
    prepared: &PreparedGeneration,
    attempt_id: &str,
    store: &R,
    inject_post_rename_sync_failure: bool,
    pause_at_temp_publication: bool,
) -> CoreResult<StageReceipt>
where
    R: KnowledgeRepository + KnowledgeGraphRepository + CodeGenerationStaging + ?Sized,
{
    let key = generation_manifest_key(binding, prepared, attempt_id)?;
    let expected = receipt(binding, prepared, attempt_id)?;
    // Retain the verified private-directory capability: the publication durability
    // boundary syncs through its platform-aware directory handle (an ordinary
    // read-only open cannot open a Windows directory at all).
    let private_root = crate::git_source::private_directory(root)?;
    let final_path = root.join(format!("{key}.json"));
    let lock_path = root.join(format!("{key}.lock"));
    let mut lock_options = OpenOptions::new();
    lock_options
        .read(true)
        .write(true)
        .create(true)
        .truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        lock_options.custom_flags(libc::O_NOFOLLOW).mode(0o600);
    }
    let lock = lock_options.open(lock_path).map_err(io_error)?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // A kernel-held nonblocking lock is released after a crash; no stranded lease file.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(CoreError::Conflict {
                reason: "generation staging is already owned by another process".into(),
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::ERROR_LOCK_VIOLATION;
        use windows_sys::Win32::Storage::FileSystem::{
            LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
        };
        use windows_sys::Win32::System::IO::OVERLAPPED;
        // Kernel-held exclusive byte-range lock on the whole lock file; the OS releases
        // it when the handle closes, including after a crash. Equivalent exclusion to
        // the Unix flock across and within processes (separate handles conflict).
        // LockFileEx requires a valid OVERLAPPED even on this synchronous handle;
        // a zero-initialized structure at offset zero spans the whole locked range and
        // only needs to outlive this fail-immediately call, which returns before it.
        let mut overlapped = OVERLAPPED::default();
        if unsafe {
            LockFileEx(
                lock.as_raw_handle(),
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                u32::MAX,
                u32::MAX,
                &mut overlapped as *mut OVERLAPPED,
            )
        } == 0
        {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION as i32) {
                return Err(CoreError::Conflict {
                    reason: "generation staging is already owned by another process".into(),
                });
            }
            return Err(io_error(error));
        }
    }
    if let Ok(bytes) = fs::read(&final_path) {
        let prior: StageReceipt = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if prior != expected {
            return Err(invalid());
        }
        // An idempotent retry re-establishes the required publication durability
        // before reporting readiness: a prior writer may have crashed between its
        // rename and the directory sync.
        private_root.sync().map_err(io_error)?;
        return Ok(prior);
    }
    write_prepared_generation(store, prepared).await?;
    for (record_id, lineage) in &prepared.lineage {
        store
            .stage_lineage(&binding.scope, record_id, lineage)
            .await?;
    }
    for doc in &prepared.documents {
        store.stage_source(&binding.scope, &doc.source).await?;
    }
    // Unique per-call temporary names avoid same-process collisions; while this
    // exclusive lock is held no legitimate writer for this key exists, so every
    // matching leftover is a killed writer's temporary with no receipt authority.
    let temp = root.join(format!(
        ".{key}.{}.{}.tmp",
        std::process::id(),
        TEMP_MANIFEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let stale_prefix = format!(".{key}.");
    for entry in fs::read_dir(root).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(&stale_prefix)
            && name.ends_with(".tmp")
            && entry.path() != temp
            && fs::remove_file(entry.path()).is_err()
        {
            return Err(io_error(std::io::Error::other(
                "stale generation-manifest temporary could not be removed under the lock",
            )));
        }
    }
    let pending = TemporaryManifest(temp.clone());
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(io_error)?;
    output
        .write_all(&serde_json::to_vec(&expected).map_err(|_| invalid())?)
        .map_err(io_error)?;
    output.sync_all().map_err(io_error)?;
    if pause_at_temp_publication {
        // Hold the exclusive lock and the published temporary while a contender or a
        // crash is exercised by the caller (bounded well below any test timeout).
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
    fs::rename(&temp, &final_path).map_err(io_error)?;
    if inject_post_rename_sync_failure {
        return Err(io_error(std::io::Error::other(
            "injected post-rename directory sync failure",
        )));
    }
    private_root.sync().map_err(io_error)?;
    drop(pending);
    drop(lock);
    Ok(expected)
}
