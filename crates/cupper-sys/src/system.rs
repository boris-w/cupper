//! Facts about the machine and the user that the host reads at startup, and
//! the host's own stdio (design 5.2.13).

use std::ffi::{CStr, OsString};
use std::fs::OpenOptions;
use std::io;
use std::os::fd::{AsRawFd, IntoRawFd};
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr;

use crate::{Error, Result};

/// The UUID macOS gives the current boot (`kern.bootsessionuuid`), in its
/// canonical upper-case form. It changes on every boot, so the host can tell
/// records of an earlier boot from its own (`data_epoch`, design 5.2.8).
pub fn boot_session_uuid() -> Result<String> {
    let uuid = sysctl_string(c"kern.bootsessionuuid")?;
    if !is_uuid(&uuid) {
        return Err(Error::Unexpected("kern.bootsessionuuid"));
    }
    Ok(uuid)
}

/// The machine's host name (`kern.hostname`), such as `name.local`; the logs
/// redact it (design 5.11.2).
pub fn host_name() -> Result<String> {
    sysctl_string(c"kern.hostname")
}

/// A string-valued sysctl.
fn sysctl_string(name: &CStr) -> Result<String> {
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
    CStr::from_bytes_until_nul(&buf)
        .ok()
        .and_then(|s| s.to_str().ok())
        .map(str::to_owned)
        .ok_or(Error::Unexpected("a sysctl string"))
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

/// Points fds 0, 1 and 2 of this process at `/dev/null`, closing whatever
/// they referred to. The host does so once it has read its startup payload
/// (design 5.8.1, 5.2.13).
pub fn redirect_stdio_to_null() -> Result<()> {
    let null = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")
        .map_err(|source| Error::Os {
            call: "open(/dev/null)",
            source,
        })?;
    let raw = null.as_raw_fd();
    for fd in 0..=2 {
        if fd == raw {
            continue;
        }
        loop {
            // SAFETY: dup2 takes plain fds and `null` stays open meanwhile.
            // Replacing 0–2 closes what they referred to; Rust code only
            // reaches those through the fd numbers, which stay valid.
            if unsafe { libc::dup2(raw, fd) } >= 0 {
                break;
            }
            let source = io::Error::last_os_error();
            if source.kind() != io::ErrorKind::Interrupted {
                return Err(Error::Os {
                    call: "dup2",
                    source,
                });
            }
        }
    }
    if raw <= 2 {
        // `/dev/null` was opened as one of 0–2 (one was closed): keep it.
        let _ = null.into_raw_fd();
    }
    Ok(())
}

/// Makes this process the leader of a new session and process group, with no
/// controlling terminal; its pid is their id. Fails for a process that leads
/// a group already.
pub fn setsid() -> Result<i32> {
    // SAFETY: setsid takes no arguments and touches no memory of ours.
    let sid = unsafe { libc::setsid() };
    if sid < 0 {
        return Err(Error::last_os("setsid"));
    }
    Ok(sid)
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
    fn host_name_is_not_empty() {
        assert!(!host_name().unwrap().is_empty());
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
