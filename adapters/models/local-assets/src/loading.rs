//! Offline loading checks the exact bytes subsequently passed to inference.
use crate::manifest::digest;
use crate::{AdmittedModel, ModelManifest};
use engram_runtime::{CoreError, CoreResult};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Read,
    path::{Path, PathBuf},
};

pub struct VerifiedModelAssets {
    manifest: ModelManifest,
    identity: String,
    files: BTreeMap<String, Vec<u8>>,
}
impl VerifiedModelAssets {
    pub fn manifest(&self) -> &ModelManifest {
        &self.manifest
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn take(&mut self, path: &str) -> CoreResult<Vec<u8>> {
        self.files
            .remove(path)
            .ok_or_else(|| invalid("required verified model asset is absent"))
    }
}
fn invalid(reason: &str) -> CoreError {
    CoreError::ValidationFailed {
        reason: reason.into(),
    }
}
fn unavailable() -> CoreError {
    CoreError::ProviderUnavailable {
        provider: "local model assets missing; provision admitted model assets explicitly".into(),
    }
}
fn io(error: std::io::Error) -> CoreError {
    if error.kind() == std::io::ErrorKind::NotFound {
        unavailable()
    } else {
        CoreError::Adapter {
            adapter: "local-model-assets".into(),
            message: error.to_string(),
        }
    }
}

/// Resolved once at construction, never from per-query input. Runtime loading
/// does not create directories, provision files or make network requests.
pub fn default_cache_root() -> CoreResult<PathBuf> {
    if let Some(root) = std::env::var_os("ENGRAM_MODEL_CACHE") {
        if root.is_empty() {
            return Err(invalid("ENGRAM_MODEL_CACHE must be a nonempty directory"));
        }
        return Ok(PathBuf::from(root));
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(".cache/engram/models"))
        .ok_or_else(unavailable)
}

pub fn load(model: AdmittedModel, root: &Path) -> CoreResult<VerifiedModelAssets> {
    let (manifest, identity) = model.manifest()?;
    let root = root.canonicalize().map_err(io)?;
    let directory = root.join(model.slug()).join(&manifest.revision);
    let files = load_files(&manifest, &root, &directory)?;
    Ok(VerifiedModelAssets {
        manifest,
        identity,
        files,
    })
}
fn load_files(
    manifest: &ModelManifest,
    root: &Path,
    directory: &Path,
) -> CoreResult<BTreeMap<String, Vec<u8>>> {
    manifest.validate()?;
    let root = root.canonicalize().map_err(io)?;
    let mut files = BTreeMap::new();
    for asset in &manifest.assets {
        let path = directory.join(&asset.path).canonicalize().map_err(io)?;
        if !path.starts_with(&root) {
            return Err(invalid("model asset escapes operator cache root"));
        }
        if !path.metadata().map_err(io)?.is_file() {
            return Err(invalid("model asset must be a regular file"));
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
        }
        let file = options.open(path).map_err(io)?;
        let metadata = file.metadata().map_err(io)?;
        if !metadata.is_file() || metadata.len() != asset.bytes {
            return Err(invalid("model asset size or file type is incompatible"));
        }
        let mut bytes = Vec::with_capacity(
            usize::try_from(asset.bytes)
                .map_err(|_| invalid("model asset exceeds address space"))?,
        );
        file.take(asset.bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        if bytes.len() as u64 != asset.bytes || digest(&bytes) != asset.sha256 {
            return Err(invalid("model asset digest is incompatible"));
        }
        files.insert(asset.path.clone(), bytes);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{ModelAsset, ModelManifest};
    fn manifest() -> ModelManifest {
        let (mut manifest, _) = crate::AdmittedModel::BgeSmallEnV15.manifest().unwrap();
        manifest.assets = vec![ModelAsset {
            path: "model.onnx".into(),
            bytes: 5,
            sha256: digest(b"model"),
        }];
        manifest
    }
    #[test]
    fn verified_bytes_are_owned_and_size_or_hash_changes_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("model.onnx"), b"model").unwrap();
        let assets = load_files(&manifest(), root.path(), root.path()).unwrap();
        std::fs::write(root.path().join("model.onnx"), b"other").unwrap();
        assert_eq!(assets["model.onnx"], b"model");
        assert!(load_files(&manifest(), root.path(), root.path()).is_err());
        std::fs::write(root.path().join("model.onnx"), b"model plus").unwrap();
        assert!(load_files(&manifest(), root.path(), root.path()).is_err());
    }
    #[test]
    fn path_traversal_and_undeclared_external_data_are_rejected() {
        let mut manifest = manifest();
        manifest.assets[0].path = "../outside.onnx".into();
        assert!(manifest.validate().is_err());
        manifest.assets[0].path = "model.onnx".into();
        manifest.external_data.push("missing-data.bin".into());
        assert!(manifest.validate().is_err());
    }
    #[cfg(unix)]
    #[test]
    fn fifo_asset_is_rejected_without_blocking_startup() {
        use std::{
            os::unix::ffi::OsStrExt,
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        if let Some(root) = std::env::var_os("ENGRAM_TEST_ASSET_FIFO_ROOT") {
            let root = PathBuf::from(root);
            assert!(load_files(&manifest(), &root, &root).is_err());
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let path =
            std::ffi::CString::new(root.path().join("model.onnx").as_os_str().as_bytes()).unwrap();
        // Isolated test fixture, never runtime model loading.
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "loading::tests::fifo_asset_is_rejected_without_blocking_startup",
            ])
            .env("ENGRAM_TEST_ASSET_FIFO_ROOT", root.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("FIFO blocked offline model loading");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[cfg(unix)]
    #[test]
    fn symlink_outside_operator_cache_is_rejected_even_for_matching_bytes() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = outside.path().join("model.onnx");
        std::fs::write(&path, b"model").unwrap();
        std::os::unix::fs::symlink(path, root.path().join("model.onnx")).unwrap();
        assert!(load_files(&manifest(), root.path(), root.path()).is_err());
    }
}
