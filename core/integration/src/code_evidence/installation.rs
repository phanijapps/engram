//! Explicit trusted-operator installation; no discovery or request-time reload.
use engram_domain::CodeCalibrationArtifact;
use engram_runtime::{CoreError, CoreResult};
use std::{io::Read, path::Path};

pub(crate) fn load(path: Option<&Path>) -> CoreResult<Option<CodeCalibrationArtifact>> {
    let Some(path) = path else { return Ok(None) };
    let invalid = || CoreError::InvalidRequest {
        reason: "code calibration must be a readable regular JSON file of at most 64 KiB".into(),
    };
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    // Avoid FIFO hangs and symlink swaps between path validation and open.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    if !std::fs::symlink_metadata(path)
        .map_err(|_| invalid())?
        .is_file()
    {
        return Err(invalid());
    }
    let file = options.open(path).map_err(|_| invalid())?;
    let metadata = file.metadata().map_err(|_| invalid())?;
    if !metadata.is_file() || metadata.len() > 65536 {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() > 65536 {
        return Err(invalid());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_artifact_never_silently_disappears_or_reads_nonregular_inputs() {
        let root = tempfile::tempdir().unwrap();
        assert!(load(None).unwrap().is_none());
        assert!(load(Some(root.path())).is_err());
        assert!(load(Some(&root.path().join("missing"))).is_err());
        let file = root.path().join("artifact.json");
        std::fs::write(&file, b"{}").unwrap();
        assert!(load(Some(&file)).is_err());
        std::fs::write(&file, vec![b' '; 65537]).unwrap();
        assert!(load(Some(&file)).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = root.path().join("link");
            symlink(&file, &link).unwrap();
            assert!(load(Some(&link)).is_err());
            let fifo = root.path().join("fifo");
            let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            assert!(load(Some(&fifo)).is_err());
        }
    }
}
