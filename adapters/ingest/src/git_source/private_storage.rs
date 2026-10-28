//! Owner-private bootstrap plus capability-relative data and cleanup operations.
#[cfg(unix)]
use cap_std::fs::DirBuilder;
use cap_std::fs::{Dir, Metadata, MetadataExt, OpenOptions};
use engram_knowledge::{CoreError, CoreResult};
use serde::Serialize;
use std::{
    ffi::OsString,
    io,
    path::{Component, Path, PathBuf},
};
#[cfg(windows)]
#[path = "windows_storage.rs"]
mod platform;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DirectoryId {
    volume: u64,
    index: [u8; 16],
}
#[cfg(unix)]
fn identity(meta: &Metadata) -> io::Result<DirectoryId> {
    Ok(DirectoryId {
        volume: meta.dev(),
        index: (meta.ino() as u128).to_le_bytes(),
    })
}
fn directory_identity(dir: &Dir) -> io::Result<DirectoryId> {
    #[cfg(unix)]
    {
        identity(&dir.dir_metadata()?)
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        platform::identity_handle(dir.as_raw_handle())
    }
}
pub(crate) fn directory_identity_of(dir: &Dir) -> io::Result<DirectoryId> {
    directory_identity(dir)
}
/// Strip Windows verbatim-prefix conventions (`\\?\` and `\\?\UNC\`) so identity
/// preflight reopens go through ordinary host path resolution, where aliasing
/// (short names, case, Unicode forms) actually applies. Unix paths pass through.
pub(crate) fn ordinary_resolution_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
        const VERBATIM: [u16; 4] = [92, 92, 63, 92];
        const VERBATIM_UNC: [u16; 8] = [92, 92, 63, 92, 85, 78, 67, 92];
        if units.starts_with(&VERBATIM_UNC) {
            let rest = units[8..].to_vec();
            units = vec![92, 92];
            units.extend(rest);
        } else if units.starts_with(&VERBATIM) {
            units = units[4..].to_vec();
        }
        return PathBuf::from(std::ffi::OsString::from_wide(&units));
    }
    #[cfg(unix)]
    {
        path.to_path_buf()
    }
}
/// Identity of a freshly created placeholder reopened through ordinary native path
/// resolution, where host aliasing (short names, case, Unicode forms) applies.
pub(crate) fn native_file_identity(path: &Path) -> io::Result<DirectoryId> {
    #[cfg(unix)]
    {
        std_native_identity(path)
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        let file = std::fs::File::open(path)?;
        platform::identity_handle(file.as_raw_handle())
    }
}
/// Directory variant; Windows requires backup semantics to open a directory handle.
pub(crate) fn native_directory_identity(path: &Path) -> io::Result<DirectoryId> {
    #[cfg(unix)]
    {
        std_native_identity(path)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        platform::identity_handle(file.as_raw_handle())
    }
}
#[cfg(unix)]
fn std_native_identity(path: &Path) -> io::Result<DirectoryId> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path)?;
    Ok(DirectoryId {
        volume: meta.dev(),
        index: (meta.ino() as u128).to_le_bytes(),
    })
}
pub(crate) fn file_identity(file: &cap_std::fs::File) -> io::Result<DirectoryId> {
    #[cfg(unix)]
    {
        identity(&file.metadata()?)
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        platform::identity_handle(file.as_raw_handle())
    }
}
fn named_identity(parent: &Dir, name: &Path) -> io::Result<DirectoryId> {
    #[cfg(unix)]
    {
        identity(&parent.symlink_metadata(name)?)
    }
    #[cfg(windows)]
    {
        use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt, OpenOptionsMaybeDirExt};
        let mut opts = OpenOptions::new();
        opts.read(true).maybe_dir(true).follow(FollowSymlinks::No);
        let file = parent.open_with(name, &opts)?;
        reject_link(&file.metadata()?)?;
        file_identity(&file)
    }
}
fn storage_error() -> CoreError {
    CoreError::InvalidRequest {
        reason: "source storage must be an owned private directory without links or reparse points"
            .into(),
    }
}
fn verify(dir: &Dir, strict_root: bool) -> io::Result<()> {
    let meta = dir.dir_metadata()?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(io::Error::other("not a real directory"));
    }
    #[cfg(unix)]
    {
        let _ = strict_root;
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
            return Err(io::Error::other("directory is not owner-private"));
        }
    }
    #[cfg(windows)]
    {
        platform::verify(dir, strict_root)?;
    }
    Ok(())
}
fn reject_link(meta: &Metadata) -> io::Result<()> {
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(io::Error::other("directory link rejected"));
    }
    #[cfg(windows)]
    {
        if meta.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(io::Error::other("directory reparse point rejected"));
        }
    }
    Ok(())
}
pub(crate) struct PrivateDirectory {
    pub(crate) dir: Dir,
    pub(crate) path: PathBuf,
}
impl PrivateDirectory {
    pub(crate) fn sync(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            // Dir::open_dir may return an O_PATH capability; reopen with real read access for fsync.
            use cap_std::fs::OpenOptionsExt;
            let mut options = OpenOptions::new();
            options
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW);
            self.dir.open_with(".", &options)?.sync_all()
        }
        #[cfg(windows)]
        {
            use cap_fs_ext::OpenOptionsMaybeDirExt;
            use cap_std::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            };
            let mut opts = OpenOptions::new();
            opts.maybe_dir(true)
                .read(true)
                .write(true)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
            self.dir.open_with(".", &opts)?.sync_all()
        }
    }
    pub(crate) fn child(&self, path: &Path) -> io::Result<Self> {
        if path.is_absolute()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(io::Error::other("non-relative private directory"));
        }
        reject_link(&self.dir.symlink_metadata(path)?)?;
        let dir = self.dir.open_dir(path)?;
        verify(&dir, false)?;
        Ok(Self {
            dir,
            path: self.path.join(path),
        })
    }
    pub(crate) fn ensure_subdirectory(&self, path: &Path) -> io::Result<Self> {
        let mut current = Self {
            dir: self.dir.try_clone()?,
            path: self.path.clone(),
        };
        for part in path.components() {
            let Component::Normal(name) = part else {
                return Err(io::Error::other("non-relative source directory"));
            };
            match create_child_directory(&current, name) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            current = current.child(Path::new(name))?;
        }
        Ok(current)
    }
    pub(crate) fn publish(&self, from: &std::ffi::OsStr, to: &str) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::{fd::AsRawFd, unix::ffi::OsStrExt};
            let from = std::ffi::CString::new(from.as_bytes())
                .map_err(|_| io::Error::other("invalid publication name"))?;
            let to = std::ffi::CString::new(to)
                .map_err(|_| io::Error::other("invalid publication name"))?;
            #[cfg(target_os = "linux")]
            let result = unsafe {
                libc::renameat2(
                    self.dir.as_raw_fd(),
                    from.as_ptr(),
                    self.dir.as_raw_fd(),
                    to.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            };
            #[cfg(target_os = "macos")]
            let result = unsafe {
                libc::renameatx_np(
                    self.dir.as_raw_fd(),
                    from.as_ptr(),
                    self.dir.as_raw_fd(),
                    to.as_ptr(),
                    libc::RENAME_EXCL,
                )
            };
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "atomic no-replace publication unavailable",
            ));
            if result != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
            let from: Vec<u16> = self
                .path
                .join(from)
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            let to: Vec<u16> = self
                .path
                .join(to)
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }
    pub(crate) fn create_file(&self, name: &Path) -> io::Result<cap_std::fs::File> {
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        self.dir.open_with(name, &opts)
    }
}
fn create_child_directory(parent: &PrivateDirectory, name: &std::ffi::OsStr) -> io::Result<()> {
    #[cfg(unix)]
    {
        use cap_std::fs::DirBuilderExt;
        let mut builder = DirBuilder::new();
        builder.mode(0o700);
        parent.dir.create_dir_with(name, &builder)
    }
    #[cfg(windows)]
    {
        platform::create(&parent.path.join(name))
    }
}
/// Bootstrap only the configured root; all data operations below it use capability-relative handles.
pub(crate) fn private_directory(path: &Path) -> CoreResult<PrivateDirectory> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| storage_error())?
            .join(path)
    };
    let name = absolute.file_name().ok_or_else(storage_error)?;
    let parent_path = absolute.parent().ok_or_else(storage_error)?;
    if !parent_path.exists() {
        let _parent = private_directory(parent_path)?;
    }
    let canonical_parent = std::fs::canonicalize(parent_path).map_err(|_| storage_error())?;
    let parent = Dir::open_ambient_dir(&canonical_parent, cap_std::ambient_authority())
        .map_err(|_| storage_error())?;
    let root = canonical_parent.join(name);
    let created = match parent.symlink_metadata(name) {
        Ok(meta) => {
            reject_link(&meta).map_err(|_| storage_error())?;
            false
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            #[cfg(unix)]
            {
                use cap_std::fs::DirBuilderExt;
                let mut builder = DirBuilder::new();
                builder.mode(0o700);
                parent
                    .create_dir_with(name, &builder)
                    .map_err(|_| storage_error())?;
            }
            #[cfg(windows)]
            {
                platform::create(&root).map_err(|_| storage_error())?;
            }
            true
        }
        Err(_) => return Err(storage_error()),
    };
    // Establish cleanup custody immediately after successful mkdir, before open/permission verification.
    let outcome: io::Result<PrivateDirectory> = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::*;
            options
                .write(true)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        }
        let file = options.open(&root)?;
        let dir = Dir::from_std_file(file);
        verify(&dir, true)?;
        let named = parent.symlink_metadata(name)?;
        reject_link(&named)?;
        if named_identity(&parent, Path::new(name))? != directory_identity(&dir)? {
            return Err(io::Error::other("directory identity changed"));
        }
        Ok(PrivateDirectory {
            dir,
            path: root.clone(),
        })
    })();
    match outcome {
        Ok(dir) => Ok(dir),
        Err(error) => {
            if created {
                if let Err(cleanup) = parent.remove_dir(name) {
                    return Err(pending_error(
                        "private-directory-bootstrap",
                        "remove-empty-directory",
                        &root,
                        None,
                        &error,
                        &cleanup,
                    ));
                }
            }
            Err(storage_error())
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct CleanupPending {
    pub state: &'static str,
    pub retry_owner: &'static str,
    pub action: &'static str,
    pub target: String,
    pub target_encoding: &'static str,
    #[serde(serialize_with = "serialize_private_native_units")]
    pub target_native_units: Vec<u32>,
    pub directory_id: Option<DirectoryId>,
    pub operation_failure: String,
    pub cleanup_failure: String,
}
fn serialize_private_native_units<S: serde::Serializer>(
    units: &[u32],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("<private native-units={units:?}>"))
}
fn native_path_units(path: &Path) -> Vec<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str()
            .as_bytes()
            .iter()
            .map(|&x| x as u32)
            .collect()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().map(u32::from).collect()
    }
}
fn pending_error(
    owner: &'static str,
    action: &'static str,
    path: &Path,
    id: Option<DirectoryId>,
    original: &impl std::fmt::Display,
    cleanup: &io::Error,
) -> CoreError {
    let pending = CleanupPending {
        state: "cleanup_pending",
        retry_owner: owner,
        action,
        target: path.to_string_lossy().into_owned(),
        #[cfg(unix)]
        target_encoding: "unix-bytes",
        #[cfg(windows)]
        target_encoding: "windows-utf16",
        target_native_units: native_path_units(path),
        directory_id: id,
        operation_failure: original.to_string(),
        cleanup_failure: format!("{:?}", cleanup.kind()),
    };
    CoreError::Adapter {
        adapter: "engram-source-custody".into(),
        message: serde_json::to_string(&pending)
            .unwrap_or_else(|_| "cleanup_pending: custody serialization failed".into()),
    }
}
pub(crate) struct OwnedAttempt {
    parent: PrivateDirectory,
    name: OsString,
    id: Option<DirectoryId>,
    pub(crate) directory: Option<PrivateDirectory>,
    owner: &'static str,
    published: bool,
}
impl OwnedAttempt {
    pub(crate) fn create(
        parent: &PrivateDirectory,
        name: &str,
        owner: &'static str,
    ) -> CoreResult<Self> {
        Self::create_checked(parent, name, owner, |_, _| Ok(()))
    }
    fn create_checked(
        parent: &PrivateDirectory,
        name: &str,
        owner: &'static str,
        after_create: impl FnOnce(&Dir, &std::ffi::OsStr) -> io::Result<()>,
    ) -> CoreResult<Self> {
        if Path::new(name).components().count() != 1
            || !matches!(
                Path::new(name).components().next(),
                Some(Component::Normal(_))
            )
        {
            return Err(storage_error());
        }
        // Prepare every allocation/handle before mkdir so the guard is established without a fallible gap.
        let mut guard = Self {
            parent: PrivateDirectory {
                dir: parent.dir.try_clone().map_err(|_| storage_error())?,
                path: parent.path.clone(),
            },
            name: name.into(),
            id: None,
            directory: None,
            owner,
            published: false,
        };
        create_child_directory(&guard.parent, &guard.name).map_err(|_| storage_error())?;
        let result: io::Result<()> = (|| {
            let meta = guard.parent.dir.symlink_metadata(&guard.name)?;
            reject_link(&meta)?;
            guard.id = Some(named_identity(&guard.parent.dir, Path::new(&guard.name))?);
            after_create(&guard.parent.dir, &guard.name)?;
            let dir = guard.parent.child(Path::new(&guard.name))?;
            guard.directory = Some(dir);
            Ok(())
        })();
        if let Err(error) = result {
            return guard
                .finish::<()>(Err(CoreError::Adapter {
                    adapter: "engram-source-storage".into(),
                    message: format!("private attempt verification failed: {error}"),
                }))
                .map(|_| unreachable!());
        }
        Ok(guard)
    }
    pub(crate) fn finish<T>(self, result: CoreResult<T>) -> CoreResult<T> {
        self.finish_with_sync(result, |parent| parent.sync())
    }
    fn finish_with_sync<T>(
        mut self,
        result: CoreResult<T>,
        synchronize: impl FnOnce(&PrivateDirectory) -> io::Result<()>,
    ) -> CoreResult<T> {
        if result.is_ok() {
            return result;
        }
        self.directory.take();
        let original = match result {
            Err(error) => error,
            Ok(_) => unreachable!(),
        };
        let target = self.parent.path.join(&self.name);
        if self.published {
            return Err(pending_error(
                self.owner,
                "complete-retained-publication-no-delete",
                &target,
                self.id,
                &original,
                &io::Error::other("publication durability pending"),
            ));
        }
        let removed = (|| {
            let meta = self.parent.dir.symlink_metadata(&self.name)?;
            reject_link(&meta)?;
            if self.id.as_ref() != Some(&named_identity(&self.parent.dir, Path::new(&self.name))?) {
                return Err(io::Error::other(
                    "failed attempt identity changed; inspection required",
                ));
            }
            self.parent.dir.remove_dir_all(&self.name)
        })();
        if let Err(cleanup) = removed {
            return Err(pending_error(
                self.owner,
                "remove-failed-attempt",
                &target,
                self.id,
                &original,
                &cleanup,
            ));
        }
        if let Err(cleanup) = synchronize(&self.parent) {
            return Err(pending_error(
                self.owner,
                "sync-parent-after-removal",
                &self.parent.path,
                directory_identity(&self.parent.dir).ok(),
                &original,
                &cleanup,
            ));
        }
        Err(original)
    }
    pub(crate) fn release_directory(&mut self) -> PrivateDirectory {
        self.directory.take().expect("owned attempt directory")
    }
    pub(crate) fn mark_published(&mut self, name: &str) {
        self.name = name.into();
        self.published = true;
    }
    pub(crate) fn name(&self) -> &std::ffi::OsStr {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn root() -> (tempfile::TempDir, PrivateDirectory) {
        let tmp = tempfile::tempdir().unwrap();
        let root = private_directory(&tmp.path().join("managed")).unwrap();
        (tmp, root)
    }
    fn failed() -> CoreError {
        CoreError::InvalidRequest {
            reason: "injected owned-fixture failure".into(),
        }
    }
    fn pending(error: CoreError) -> serde_json::Value {
        let CoreError::Adapter { adapter, message } = error else {
            panic!("expected custody error")
        };
        assert_eq!(adapter, "engram-source-custody");
        serde_json::from_str(&message).unwrap()
    }
    #[test]
    fn task_008_private_directory_identity_and_immediate_permission_failure_custody() {
        let (_tmp, root) = root();
        assert!(private_directory(&root.path).is_ok());
        let result =
            OwnedAttempt::create_checked(&root, "permission-failure", "fixture-owner", |_, _| {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            });
        assert!(result.is_err());
        assert_eq!(root.dir.entries().unwrap().count(), 0);
        let attempt = OwnedAttempt::create(&root, "partial", "fixture-owner").unwrap();
        attempt
            .directory
            .as_ref()
            .unwrap()
            .create_file(Path::new("objects.pack"))
            .unwrap()
            .write_all(b"partial")
            .unwrap();
        assert!(attempt.finish::<()>(Err(failed())).is_err());
        assert_eq!(root.dir.entries().unwrap().count(), 0);
    }
    #[cfg(unix)]
    #[test]
    fn task_008_non_utf8_native_unit_serialization_is_lossless_and_redacted() {
        use std::os::unix::ffi::OsStringExt;
        // Synthetic lossless serialization: no filesystem entry is required, so this
        // runs on every Unix host regardless of what byte sequences its mount accepts.
        let name = OsString::from_vec(b"fixture-root/private-\xff".to_vec());
        let target_os = std::env::temp_dir().join(&name);
        let target = target_os.as_path();
        let error = pending_error(
            "fixture-owner",
            "sync-parent-after-removal",
            target,
            None,
            &failed(),
            &io::Error::other("injected sync failure"),
        );
        let redacted = error.to_redacted().to_string();
        assert!(!redacted.contains("native-units="));
        assert!(!redacted.contains(target.to_string_lossy().as_ref()));
        let detail = pending(error);
        let expected: Vec<u32> = {
            use std::os::unix::ffi::OsStrExt;
            target_os
                .as_os_str()
                .as_bytes()
                .iter()
                .map(|&x| x as u32)
                .collect()
        };
        assert_eq!(
            detail["target_native_units"].as_str().unwrap(),
            &format!("<private native-units={expected:?}>")
        );
        assert_eq!(detail["target_encoding"], "unix-bytes");
    }
    #[cfg(unix)]
    #[test]
    fn task_008_non_utf8_filesystem_roundtrip_only_where_representable() {
        use std::os::unix::ffi::OsStringExt;
        let temp = tempfile::tempdir().unwrap();
        let dir = temp
            .path()
            .join(OsString::from_vec(b"private-\xff".to_vec()));
        // Direct disposable filesystem probe preserving the actual OS outcome; unrelated
        // permission, ownership or capability failures must fail, never skip silently.
        if let Err(error) = std::fs::create_dir(&dir) {
            // Native observation (run 40758578615): macOS rejects the name with
            // EILSEQ (raw os_error 92), which maps to Uncategorized, not InvalidInput.
            // Accept only the illegal-sequence/invalid-name family; permission,
            // ownership or capability failures must still fail the test.
            let unsupported_name = matches!(error.kind(), std::io::ErrorKind::InvalidInput)
                || matches!(error.raw_os_error(), Some(22 | 84 | 92));
            assert!(
                unsupported_name,
                "unexpected probe failure kind={:?} os_error={:?}",
                error.kind(),
                error.raw_os_error()
            );
            assert!(!dir.exists());
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
            return;
        }
        // The probe only answers representability; remove it so private_directory
        // bootstraps the real owner-private root itself.
        std::fs::remove_dir(&dir).unwrap();
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        let root = private_directory(&dir).unwrap();
        let attempt = OwnedAttempt::create(&root, "sync-failure", "fixture-owner").unwrap();
        let error = attempt
            .finish_with_sync::<()>(Err(failed()), |_| {
                Err(io::Error::other("injected sync failure"))
            })
            .unwrap_err();
        let redacted = error.to_redacted().to_string();
        assert!(!redacted.contains("native-units="));
        assert!(!redacted.contains(root.path.to_string_lossy().as_ref()));
        let detail = pending(error);
        let expected: Vec<u32> = {
            use std::os::unix::ffi::OsStrExt;
            root.path
                .as_os_str()
                .as_bytes()
                .iter()
                .map(|&x| x as u32)
                .collect()
        };
        assert_eq!(
            detail["target_native_units"].as_str().unwrap(),
            &format!("<private native-units={expected:?}>")
        );
        assert_eq!(detail["target_encoding"], "unix-bytes");
    }
    #[test]
    fn task_008_cleanup_sync_failure_has_exact_parent_retry_custody() {
        let (_tmp, root) = root();
        let attempt = OwnedAttempt::create(&root, "sync-failure", "fixture-owner").unwrap();
        let error = attempt
            .finish_with_sync::<()>(Err(failed()), |_| {
                Err(io::Error::other("injected sync failure"))
            })
            .unwrap_err();
        let redacted = error.to_redacted().to_string();
        assert!(!redacted.contains("native-units="));
        assert!(!redacted.contains(root.path.to_string_lossy().as_ref()));
        let pending = pending(error);
        assert_eq!(pending["state"], "cleanup_pending");
        assert_eq!(pending["retry_owner"], "fixture-owner");
        assert_eq!(pending["action"], "sync-parent-after-removal");
        assert_eq!(pending["target"], root.path.to_str().unwrap());
        let expected: Vec<u32> = native_path_units(&root.path);
        assert_eq!(
            pending["target_native_units"].as_str().unwrap(),
            &format!("<private native-units={expected:?}>")
        );
        assert!(pending["directory_id"].is_object());
        assert_eq!(root.dir.entries().unwrap().count(), 0);
        root.sync().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn task_008_cleanup_removal_failure_is_not_silenced_or_reused() {
        use std::os::unix::fs::PermissionsExt;
        let (_tmp, root) = root();
        let attempt = OwnedAttempt::create(&root, "removal-failure", "fixture-owner").unwrap();
        let dir = attempt.directory.as_ref().unwrap();
        dir.create_file(Path::new("objects.pack"))
            .unwrap()
            .write_all(b"partial")
            .unwrap();
        let path = dir.path.clone();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o500)).unwrap();
        let error = attempt.finish::<()>(Err(failed())).unwrap_err();
        let pending = pending(error);
        assert_eq!(pending["action"], "remove-failed-attempt");
        assert_eq!(pending["target"], path.to_str().unwrap());
        assert_eq!(pending["retry_owner"], "fixture-owner");
        assert!(pending["directory_id"].is_object());
        assert!(OwnedAttempt::create(&root, "removal-failure", "fixture-owner").is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        root.dir.remove_dir_all("removal-failure").unwrap();
        root.sync().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn task_008_private_root_rejects_symlinks_and_public_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (tmp, root) = root();
        let alias = tmp.path().join("alias");
        symlink(&root.path, &alias).unwrap();
        assert!(private_directory(&alias).is_err());
        let public = tmp.path().join("public");
        std::fs::create_dir(&public).unwrap();
        std::fs::set_permissions(&public, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(private_directory(&public).is_err());
    }
    #[test]
    fn task_008_retained_publication_failure_never_grants_delete_authority() {
        let (_tmp, root) = root();
        let mut attempt = OwnedAttempt::create(&root, "before-publish", "fixture-owner").unwrap();
        let original = directory_identity(&attempt.directory.as_ref().unwrap().dir).unwrap();
        drop(attempt.release_directory());
        root.publish(attempt.name(), "retained").unwrap();
        attempt.mark_published("retained");
        let value = pending(attempt.finish::<()>(Err(failed())).unwrap_err());
        assert_eq!(value["action"], "complete-retained-publication-no-delete");
        assert_eq!(
            named_identity(&root.dir, Path::new("retained")).unwrap(),
            original
        );
        assert!(root.dir.open_dir("retained").is_ok());
    }
    #[test]
    fn task_008_crash_leftovers_require_reference_reconciliation_and_are_not_reused() {
        const ROOT: &str = "ENGRAM_OWNED_CRASH_FIXTURE_ROOT";
        if let Some(path) = std::env::var_os(ROOT) {
            let root = private_directory(Path::new(&path)).unwrap();
            let attempt =
                OwnedAttempt::create(&root, "attempt-crash-fixture", "fixture-owner").unwrap();
            attempt
                .directory
                .as_ref()
                .unwrap()
                .create_file(Path::new("objects.pack"))
                .unwrap()
                .write_all(b"part")
                .unwrap();
            std::process::exit(0);
        }
        let (_tmp, root) = root();
        let name = format!(
            "{}::task_008_crash_leftovers_require_reference_reconciliation_and_are_not_reused",
            module_path!().split_once("::").unwrap().1
        );
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture"])
            .env(ROOT, &root.path)
            .output()
            .unwrap();
        assert!(out.status.success());
        assert_eq!(root.dir.entries().unwrap().count(), 1);
        assert_eq!(
            root.dir.read("attempt-crash-fixture/objects.pack").unwrap(),
            b"part"
        );
        assert!(OwnedAttempt::create(&root, "attempt-crash-fixture", "fixture-owner").is_err());
        let fresh = OwnedAttempt::create(&root, "attempt-fresh-fixture", "fixture-owner").unwrap();
        fresh.finish(Ok(())).unwrap();
        assert_eq!(
            root.dir.read("attempt-crash-fixture/objects.pack").unwrap(),
            b"part"
        );
        // Both are owned disposable fixtures; production cleanup must separately reconcile successful references.
    }
}
