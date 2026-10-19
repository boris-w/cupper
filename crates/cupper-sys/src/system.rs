//! Facts about the machine and the user that the host reads at startup
//! (design 5.2.13).

use std::ffi::{CStr, OsString};
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr;

use crate::{Error, Result};

/// The UUID macOS gives the current boot (`kern.bootsessionuuid`), in its
/// canonical upper-case form. It changes on every boot, so the host can tell
/// records of an earlier boot from its own (`data_epoch`, design 5.2.8).
pub fn boot_session_uuid() -> Result<String> {
    let name = c"kern.bootsessionuuid";
    let mut len: libc::size_t = 0;
    // SAFETY: `name` is NUL-terminated; a null `oldp` only asks for the size,
    // which the kernel writes to `len`; no new value is set.
    let rc =
        unsafe { libc::sysctlbyname(name.as_ptr(), ptr::null_mut(), &mut len, ptr::null_mut(), 0) };
    if rc != 0 {
        return Err(Error::last_os("sysctlbyname"));
    }
    let mut buf = vec![0u8; len];
    // SAFETY: `buf` has `len` writable bytes and `len` says so; the kernel
    // writes at most that many and stores the written length back.
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(Error::last_os("sysctlbyname"));
    }
    buf.truncate(len);
    let uuid = CStr::from_bytes_until_nul(&buf)
        .ok()
        .and_then(|s| s.to_str().ok())
        .filter(|s| is_uuid(s))
        .ok_or(Error::Unexpected("kern.bootsessionuuid"))?;
    Ok(uuid.to_owned())
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// The per-user temporary directory (`confstr(_CS_DARWIN_USER_TEMP_DIR)`).
/// The system creates it with mode 0700 for this user only, so no other user
/// can claim a path in it first (design 5.2.13, socket path fallback).
pub fn user_temp_dir() -> Result<PathBuf> {
    let name = libc::_CS_DARWIN_USER_TEMP_DIR;
    // SAFETY: a null buffer of length 0 only asks for the size.
    let len = unsafe { libc::confstr(name, ptr::null_mut(), 0) };
    if len == 0 {
        return Err(Error::last_os("confstr"));
    }
    let mut buf = vec![0u8; len];
    // SAFETY: `buf` has `len` writable bytes; confstr writes at most `len`
    // bytes including the terminating NUL.
    let needed = unsafe { libc::confstr(name, buf.as_mut_ptr().cast(), len) };
    if needed == 0 {
        return Err(Error::last_os("confstr"));
    }
    if needed > len {
        // The value grew between the two calls; it never does in practice.
        return Err(Error::Unexpected("confstr(_CS_DARWIN_USER_TEMP_DIR)"));
    }
    buf.truncate(needed - 1);
    let dir = PathBuf::from(OsString::from_vec(buf));
    if !dir.is_absolute() {
        return Err(Error::Unexpected("confstr(_CS_DARWIN_USER_TEMP_DIR)"));
    }
    Ok(dir)
}

/// The real user ID of this process.
pub fn current_uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use super::*;

    #[test]
    fn boot_session_uuid_is_a_stable_uuid() {
        let uuid = boot_session_uuid().unwrap();
        assert!(is_uuid(&uuid), "{uuid}");
        assert_eq!(boot_session_uuid().unwrap(), uuid);
    }

    #[test]
    fn is_uuid_checks_the_canonical_form() {
        assert!(is_uuid("28E4A596-644A-4835-B57C-A088506A7CB5"));
        assert!(!is_uuid("28E4A596644A4835B57CA088506A7CB5"));
        assert!(!is_uuid("28E4A596-644A-4835-B57C-A088506A7CBZ"));
        assert!(!is_uuid("28E4A596-644A-4835-B57C-A088506A7CB"));
    }

    #[test]
    fn user_temp_dir_is_private_to_the_user() {
        let dir = user_temp_dir().unwrap();
        assert!(dir.is_absolute(), "{}", dir.display());
        let meta = std::fs::metadata(&dir).unwrap();
        assert!(meta.is_dir());
        assert_eq!(meta.uid(), current_uid());
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);
    }
}
