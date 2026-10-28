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
};

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
    let key = generation_manifest_key(binding, prepared, attempt_id)?;
    let expected = receipt(binding, prepared, attempt_id)?;
    let _private_root = crate::git_source::private_directory(root)?;
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
    if let Ok(bytes) = fs::read(&final_path) {
        let prior: StageReceipt = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if prior != expected {
            return Err(invalid());
        }
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
    let temp = root.join(format!(".{key}.{}.tmp", std::process::id()));
    // A killed writer's temporary file contains no receipt authority and may be safely replaced under the lock.
    if temp.exists() {
        fs::remove_file(&temp).map_err(io_error)?;
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
    fs::rename(&temp, &final_path).map_err(io_error)?;
    OpenOptions::new()
        .read(true)
        .open(root)
        .map_err(io_error)?
        .sync_all()
        .map_err(io_error)?;
    drop(pending);
    drop(lock);
    Ok(expected)
}
