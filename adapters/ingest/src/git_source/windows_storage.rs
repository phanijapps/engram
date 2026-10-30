//! Windows owner-only protected DACL creation and handle-bound verification.
use cap_std::fs::Dir;
use std::{
    io, mem,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};
struct Local(*mut std::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
struct User {
    storage: Vec<usize>,
}
impl User {
    fn current() -> io::Result<Self> {
        let mut token = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = unsafe { OwnedHandle::from_raw_handle(token) };
        let mut storage = vec![0usize; 256];
        let mut bytes = 0;
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                storage.as_mut_ptr().cast(),
                (storage.len() * mem::size_of::<usize>()) as u32,
                &mut bytes,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let user = Self { storage };
        if unsafe { IsValidSid(user.sid()) } == 0 {
            return Err(io::Error::other("invalid current owner SID"));
        }
        Ok(user)
    }
    fn sid(&self) -> PSID {
        unsafe { (*(self.storage.as_ptr().cast::<TOKEN_USER>())).User.Sid }
    }
    fn text(&self) -> io::Result<String> {
        let mut text = ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(self.sid(), &mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let _owned = Local(text.cast());
        let mut len = 0;
        while len < 256 && unsafe { *text.add(len) } != 0 {
            len += 1;
        }
        if len == 256 {
            return Err(io::Error::other("owner SID length invalid"));
        }
        String::from_utf16(unsafe { std::slice::from_raw_parts(text, len) })
            .map_err(|_| io::Error::other("owner SID encoding invalid"))
    }
}
pub(super) fn create(path: &Path) -> io::Result<()> {
    let owner = User::current()?.text()?;
    create_sddl(path, &format!("O:{owner}D:P(A;OICI;FA;;;{owner})"))
}
fn create_sddl(path: &Path, text: &str) -> io::Result<()> {
    let sddl: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let mut descriptor = ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let _descriptor = Local(descriptor);
    let attributes = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { CreateDirectoryW(name.as_ptr(), &attributes) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub(super) fn verify(dir: &Dir, strict_root: bool) -> io::Result<()> {
    let user = User::current()?;
    let (mut owner, mut descriptor, mut dacl) = (ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
    let result = unsafe {
        GetSecurityInfo(
            dir.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut dacl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let _descriptor = Local(descriptor);
    if owner.is_null() || unsafe { EqualSid(owner, user.sid()) } == 0 || dacl.is_null() {
        return Err(io::Error::other("directory owner or private DACL invalid"));
    }
    let (mut control, mut revision) = (0, 0);
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if strict_root && control & SE_DACL_PROTECTED == 0 {
        return Err(io::Error::other("private root DACL must be protected"));
    }
    if unsafe { (*dacl).AceCount } != 1 {
        return Err(io::Error::other(
            "private directory grants access to another principal",
        ));
    }
    let mut raw = ptr::null_mut();
    if unsafe { GetAce(dacl, 0, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let header = unsafe { &*raw.cast::<ACE_HEADER>() };
    if header.AceType != windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE as u8
        || (header.AceSize as usize) < 16
    {
        return Err(io::Error::other("invalid private-directory ACE"));
    }
    let ace = unsafe { &*raw.cast::<ACCESS_ALLOWED_ACE>() };
    let sid = ptr::addr_of!(ace.SidStart).cast_mut().cast();
    if unsafe { IsValidSid(sid) } == 0
        || unsafe { GetLengthSid(sid) } as usize > header.AceSize as usize - 8
    {
        return Err(io::Error::other("invalid private-directory ACE SID"));
    }
    let inherit = (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8;
    let permitted = inherit | if strict_root { 0 } else { INHERITED_ACE as u8 };
    if ace.Header.AceType
        != windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE as u8
        || ace.Header.AceFlags & inherit != inherit
        || ace.Header.AceFlags & !permitted != 0
        || ace.Mask != FILE_ALL_ACCESS
        || unsafe { EqualSid(ptr::addr_of!(ace.SidStart).cast_mut().cast(), user.sid()) } == 0
    {
        return Err(io::Error::other(
            "private directory ACL is not owner-only and inheritable",
        ));
    }
    Ok(())
}

/// Check the full filesystem identity on the live owned handle; never use path-only optional metadata.
pub(super) fn identity_handle(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> io::Result<super::DirectoryId> {
    let mut info = FILE_ID_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            mem::size_of::<FILE_ID_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(super::DirectoryId {
        volume: info.VolumeSerialNumber,
        index: info.FileId.Identifier,
    })
}
#[cfg(test)]
mod identity_tests {
    #[test]
    fn task_008_missing_native_identity_is_an_error_not_a_panic() {
        assert!(
            super::identity_handle(windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE).is_err()
        );
    }
}

#[cfg(test)]
mod acl_tests {
    use super::*;
    #[test]
    fn task_008_windows_private_dacl_does_not_inherit_everyone_grants() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("permissive");
        let owner = User::current().unwrap().text().unwrap();
        create_sddl(
            &parent,
            &format!("O:{owner}D:P(A;OICI;FA;;;{owner})(A;OICI;FA;;;WD)"),
        )
        .unwrap();
        assert!(super::super::private_directory(&parent).is_err());
        let private = super::super::private_directory(&parent.join("private")).unwrap();
        verify(&private.dir, true).unwrap();
        let attempt =
            super::super::OwnedAttempt::create(&private, "attempt", "fixture-owner").unwrap();
        verify(&attempt.directory.as_ref().unwrap().dir, true).unwrap();
        let child = attempt
            .directory
            .as_ref()
            .unwrap()
            .ensure_subdirectory(Path::new("nested/deep"))
            .unwrap();
        verify(&child.dir, true).unwrap();
        drop(child);
        assert!(
            attempt
                .finish::<()>(Err(engram_knowledge::CoreError::InvalidRequest {
                    reason: "fixture failure".into()
                }))
                .is_err()
        );
        assert_eq!(private.dir.entries().unwrap().count(), 0);
    }
    #[test]
    fn task_008_windows_junction_root_is_rejected_before_content() {
        let temp = tempfile::tempdir().unwrap();
        let root = super::super::private_directory(&temp.path().join("private")).unwrap();
        let alias = temp.path().join("junction");
        let result=std::process::Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command","New-Item -ItemType Junction -Path $env:ENGRAM_FIXTURE_ALIAS -Target $env:ENGRAM_FIXTURE_TARGET | Out-Null"]).env("ENGRAM_FIXTURE_ALIAS",&alias).env("ENGRAM_FIXTURE_TARGET",&root.path).output().unwrap();
        assert!(
            result.status.success(),
            "disposable unprivileged junction fixture must run"
        );
        assert!(super::super::private_directory(&alias).is_err());
        assert_eq!(root.dir.entries().unwrap().count(), 0);
    }
}
