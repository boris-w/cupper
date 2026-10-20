//! Code signing checks (design 5.8.1, SP-4b): a peer's signature against a
//! requirement built into the binary, its dynamic code-signing status, and
//! the cdhashes of running code and of files.

use std::ffi::c_void;
use std::path::Path;
use std::{fmt, io, mem, ptr};

use core_foundation::base::{CFType, CFTypeRef, OSStatus, TCFType};
use core_foundation::data::CFData;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::{CFURL, CFURLRef};
use libc::{c_int, pid_t};

use crate::peer::{AuditToken, Refusal, RefusalKind};
use crate::{Error, Result};

/// `SecCodeRef`, `SecStaticCodeRef` and `SecRequirementRef`: CF objects.
type SecObjectRef = CFTypeRef;

const K_SEC_CS_DEFAULT_FLAGS: u32 = 0;
const K_SEC_CS_SIGNING_INFORMATION: u32 = 1 << 1;

// Code signing results from <Security/CSCommon.h>.
const ERR_SEC_CS_STATIC_CODE_CHANGED: OSStatus = -67034;
const ERR_SEC_CS_REQ_FAILED: OSStatus = -67050;
const ERR_SEC_CS_UNSIGNED: OSStatus = -67062;
const ERR_SEC_CS_NO_SUCH_CODE: OSStatus = -67065;
const ERR_SEC_CS_STATIC_CODE_NOT_FOUND: OSStatus = -67068;
/// The Security framework reports a failed system call as this plus `errno`
/// (SP-4b saw 100002, `ENOENT`, for a removed executable).
const ERRNO_BASE: OSStatus = 100_000;

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecGuestAttributeAudit: CFStringRef;
    static kSecCodeInfoUnique: CFStringRef;
    static kSecCodeInfoIdentifier: CFStringRef;
    static kSecCodeInfoTeamIdentifier: CFStringRef;

    fn SecCodeCopyGuestWithAttributes(
        host: SecObjectRef,
        attributes: CFDictionaryRef,
        flags: u32,
        guest: *mut SecObjectRef,
    ) -> OSStatus;
    fn SecCodeCheckValidity(code: SecObjectRef, flags: u32, requirement: SecObjectRef) -> OSStatus;
    fn SecRequirementCreateWithString(
        text: CFStringRef,
        flags: u32,
        requirement: *mut SecObjectRef,
    ) -> OSStatus;
    fn SecStaticCodeCreateWithPath(path: CFURLRef, flags: u32, code: *mut SecObjectRef)
    -> OSStatus;
    fn SecCodeCopySigningInformation(
        code: SecObjectRef,
        flags: u32,
        information: *mut CFDictionaryRef,
    ) -> OSStatus;
}

// In libsystem_kernel, declared in XNU's private <sys/codesign.h>.
unsafe extern "C" {
    fn csops(pid: pid_t, ops: u32, useraddr: *mut c_void, usersize: usize) -> c_int;
    fn csops_audittoken(
        pid: pid_t,
        ops: u32,
        useraddr: *mut c_void,
        usersize: usize,
        token: *const [u32; 8],
    ) -> c_int;
}

const CS_OPS_STATUS: u32 = 0;
const CS_OPS_CDHASH: u32 = 5;

/// A code requirement in Apple's requirement language (design 5.8.1). The
/// text is `'static` because requirements are built into the binary, never
/// read from disk, the environment or a peer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Requirement(&'static str);

impl Requirement {
    pub const fn new(text: &'static str) -> Self {
        Self(text)
    }

    pub const fn text(self) -> &'static str {
        self.0
    }

    /// Compiles the requirement, so that a malformed one shows at startup or
    /// in a test instead of as a refusal of every peer.
    pub fn validate(self) -> Result<()> {
        self.compile().map(drop).map_err(|status| Error::Security {
            call: "SecRequirementCreateWithString",
            status,
        })
    }

    fn compile(self) -> std::result::Result<CFType, OSStatus> {
        let text = CFString::from_static_string(self.0);
        let mut requirement: SecObjectRef = ptr::null();
        // SAFETY: `text` is a live CFString; `requirement` is a valid out
        // pointer.
        let status = unsafe {
            SecRequirementCreateWithString(
                text.as_concrete_TypeRef(),
                K_SEC_CS_DEFAULT_FLAGS,
                &mut requirement,
            )
        };
        if status != 0 {
            return Err(status);
        }
        // SAFETY: on success the requirement is a new object we own.
        Ok(unsafe { CFType::wrap_under_create_rule(requirement) })
    }
}

/// A process's dynamic code-signing status (kernel `csflags`, XNU
/// `<kern/cs_blobs.h>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CodeStatus(pub u32);

impl CodeStatus {
    pub const VALID: u32 = 0x1;
    pub const GET_TASK_ALLOW: u32 = 0x4;
    pub const HARD: u32 = 0x100;
    pub const KILL: u32 = 0x200;
    pub const RUNTIME: u32 = 0x1_0000;
    pub const PLATFORM_BINARY: u32 = 0x400_0000;
    pub const DEBUGGED: u32 = 0x1000_0000;

    /// Whether every flag in `flags` is set.
    pub fn has(self, flags: u32) -> bool {
        self.0 & flags == flags
    }
}

/// Who signed a program, as its signature says (design 5.8.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigningIdentity {
    pub identifier: String,
    /// The Team ID of a developer's signature; none for ad-hoc and Apple's
    /// own code.
    pub team_id: Option<String>,
}

/// A code directory hash: names one signed build of one program.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cdhash(pub [u8; 20]);

impl fmt::Display for Cdhash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for Cdhash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cdhash({self})")
    }
}

impl AuditToken {
    /// Verifies the process this token names (design 5.8.1): its code
    /// satisfies `requirement`, with the code on disk still matching what
    /// runs, and its dynamic status has the hardened runtime and neither
    /// `get-task-allow` nor a debugger attached. Takes about a millisecond
    /// (SP-4b).
    pub fn verify(&self, requirement: Requirement) -> std::result::Result<(), Refusal> {
        let code = self.guest_code()?;
        let requirement = requirement.compile().map_err(|status| Refusal {
            kind: RefusalKind::Other,
            call: "SecRequirementCreateWithString",
            code: status.into(),
        })?;
        // SAFETY: both arguments are live objects of the expected types.
        let status = unsafe {
            SecCodeCheckValidity(
                code.as_CFTypeRef(),
                K_SEC_CS_DEFAULT_FLAGS,
                requirement.as_CFTypeRef(),
            )
        };
        if status != 0 {
            return Err(self.security_refusal("SecCodeCheckValidity", status));
        }
        let status = self.code_status()?;
        let failed = if !status.has(CodeStatus::RUNTIME) {
            Some(RefusalKind::NoRuntime)
        } else if status.has(CodeStatus::GET_TASK_ALLOW) {
            Some(RefusalKind::GetTaskAllow)
        } else if status.has(CodeStatus::DEBUGGED) {
            Some(RefusalKind::Debugged)
        } else {
            None
        };
        match failed {
            Some(kind) => Err(Refusal {
                kind,
                call: "csops(CS_OPS_STATUS)",
                code: status.0.into(),
            }),
            None => Ok(()),
        }
    }

    /// The dynamic code-signing status of the process this token names.
    pub fn code_status(&self) -> std::result::Result<CodeStatus, Refusal> {
        let mut status: u32 = 0;
        self.csops(CS_OPS_STATUS, "csops(CS_OPS_STATUS)", &mut status)?;
        Ok(CodeStatus(status))
    }

    /// The signing identity of the code the process this token names runs;
    /// `None` when it is unsigned.
    pub fn signing_identity(&self) -> std::result::Result<Option<SigningIdentity>, Refusal> {
        let code = self.guest_code()?;
        let mut info: CFDictionaryRef = ptr::null();
        // SAFETY: `code` is a live code object; `info` is a valid out pointer.
        let status = unsafe {
            SecCodeCopySigningInformation(
                code.as_CFTypeRef(),
                K_SEC_CS_SIGNING_INFORMATION,
                &mut info,
            )
        };
        if status != 0 {
            return Err(self.security_refusal("SecCodeCopySigningInformation", status));
        }
        // SAFETY: on success the dictionary is a new object we own, keyed by
        // CFStrings.
        let info: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_create_rule(info) };
        let text = |key: CFStringRef| {
            // SAFETY: a constant of the Security framework, valid while the
            // process runs.
            let key = unsafe { CFString::wrap_under_get_rule(key) };
            let value = info.find(&key)?.downcast::<CFString>()?;
            Some(value.to_string())
        };
        // SAFETY: as above, for both keys.
        let (identifier, team) = unsafe { (kSecCodeInfoIdentifier, kSecCodeInfoTeamIdentifier) };
        let identity = text(identifier).map(|identifier| SigningIdentity {
            identifier,
            team_id: text(team),
        });
        Ok(identity)
    }

    /// The cdhash of the code the process this token names runs.
    pub fn cdhash(&self) -> std::result::Result<Cdhash, Refusal> {
        let mut hash = [0u8; 20];
        self.csops(CS_OPS_CDHASH, "csops(CS_OPS_CDHASH)", &mut hash)?;
        Ok(Cdhash(hash))
    }

    /// `csops_audittoken`: answers only while the process is still the
    /// program the token names.
    fn csops<T: Copy>(
        &self,
        op: u32,
        call: &'static str,
        out: &mut T,
    ) -> std::result::Result<(), Refusal> {
        // SAFETY: `out` is a valid, writable `T` and the kernel writes at most
        // `size_of::<T>()` bytes to it; `T` is an integer or a byte array,
        // valid for any bytes. The token is read only.
        let rc = unsafe {
            csops_audittoken(
                self.pid(),
                op,
                (out as *mut T).cast(),
                mem::size_of::<T>(),
                &self.0,
            )
        };
        if rc == 0 {
            return Ok(());
        }
        let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
        Err(match errno {
            libc::ESRCH => self.vanished(call, errno.into()),
            _ => Refusal {
                kind: RefusalKind::Other,
                call,
                code: errno.into(),
            },
        })
    }

    /// The code object of the process this token names.
    fn guest_code(&self) -> std::result::Result<CFType, Refusal> {
        let bytes: Vec<u8> = self.0.iter().flat_map(|word| word.to_ne_bytes()).collect();
        let token = CFData::from_buffer(&bytes);
        // SAFETY: a constant of the Security framework, valid while the
        // process runs.
        let key = unsafe { CFString::wrap_under_get_rule(kSecGuestAttributeAudit) };
        let attributes = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), token.as_CFType())]);
        let mut code: SecObjectRef = ptr::null();
        // SAFETY: a null host asks the kernel; `attributes` is a live
        // dictionary; `code` is a valid out pointer.
        let status = unsafe {
            SecCodeCopyGuestWithAttributes(
                ptr::null(),
                attributes.as_concrete_TypeRef(),
                K_SEC_CS_DEFAULT_FLAGS,
                &mut code,
            )
        };
        if status != 0 {
            return Err(self.security_refusal("SecCodeCopyGuestWithAttributes", status));
        }
        // SAFETY: on success the code object is a new object we own.
        Ok(unsafe { CFType::wrap_under_create_rule(code) })
    }

    fn security_refusal(&self, call: &'static str, status: OSStatus) -> Refusal {
        let kind = match status {
            ERR_SEC_CS_REQ_FAILED | ERR_SEC_CS_UNSIGNED => RefusalKind::Unsatisfied,
            ERR_SEC_CS_STATIC_CODE_CHANGED | ERR_SEC_CS_STATIC_CODE_NOT_FOUND => {
                RefusalKind::CodeChanged
            }
            _ if status == ERRNO_BASE + libc::ENOENT => RefusalKind::CodeChanged,
            ERR_SEC_CS_NO_SUCH_CODE => return self.vanished(call, status.into()),
            _ if status == ERRNO_BASE + libc::ESRCH => return self.vanished(call, status.into()),
            _ => RefusalKind::Other,
        };
        Refusal {
            kind,
            call,
            code: status.into(),
        }
    }
}

/// The cdhash this process runs with. Compared with [`file_cdhash`] of its
/// own executable, it tells whether an upgrade has replaced the code on disk
/// (SP-4b; the host's self-check, design 5.2.13).
pub fn own_cdhash() -> Result<Cdhash> {
    // SAFETY: getpid has no preconditions and cannot fail.
    pid_cdhash(unsafe { libc::getpid() })
}

/// The cdhash the process `pid` runs with. Unlike a check through an audit
/// token, the pid may have been reused since it was learned: compare the
/// executable too ([`pid_path`](crate::pid_path)) before acting on it.
pub fn pid_cdhash(pid: i32) -> Result<Cdhash> {
    let mut hash = [0u8; 20];
    // SAFETY: csops takes a plain pid; the buffer has the 20 writable bytes
    // CS_OPS_CDHASH writes.
    let rc = unsafe { csops(pid, CS_OPS_CDHASH, hash.as_mut_ptr().cast(), hash.len()) };
    if rc != 0 {
        return Err(Error::last_os("csops(CS_OPS_CDHASH)"));
    }
    Ok(Cdhash(hash))
}

/// The cdhash of the signed code at `path`, a bundle or an executable, for
/// the architecture this process runs.
pub fn file_cdhash(path: &Path) -> Result<Cdhash> {
    let url = CFURL::from_path(path, path.is_dir()).ok_or(Error::InvalidArgument("path"))?;
    let mut code: SecObjectRef = ptr::null();
    // SAFETY: `url` is a live CFURL; `code` is a valid out pointer.
    let status = unsafe {
        SecStaticCodeCreateWithPath(url.as_concrete_TypeRef(), K_SEC_CS_DEFAULT_FLAGS, &mut code)
    };
    security_result("SecStaticCodeCreateWithPath", status)?;
    // SAFETY: on success the code object is a new object we own.
    let code = unsafe { CFType::wrap_under_create_rule(code) };
    let mut info: CFDictionaryRef = ptr::null();
    // SAFETY: `code` is a live static code object; `info` is a valid out
    // pointer.
    let status = unsafe {
        SecCodeCopySigningInformation(code.as_CFTypeRef(), K_SEC_CS_SIGNING_INFORMATION, &mut info)
    };
    security_result("SecCodeCopySigningInformation", status)?;
    // SAFETY: on success the dictionary is a new object we own, keyed by
    // CFStrings.
    let info: CFDictionary<CFString, CFType> =
        unsafe { CFDictionary::wrap_under_create_rule(info) };
    // SAFETY: a constant of the Security framework, valid while the process
    // runs.
    let key = unsafe { CFString::wrap_under_get_rule(kSecCodeInfoUnique) };
    // Unsigned code has no cdhash.
    let unsigned = Error::Security {
        call: "SecCodeCopySigningInformation",
        status: ERR_SEC_CS_UNSIGNED,
    };
    let unique = info
        .find(&key)
        .and_then(|value| value.downcast::<CFData>())
        .ok_or(unsigned)?;
    let hash = <[u8; 20]>::try_from(unique.bytes())
        .map_err(|_| Error::Unexpected("kSecCodeInfoUnique"))?;
    Ok(Cdhash(hash))
}

fn security_result(call: &'static str, status: OSStatus) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(Error::Security { call, status })
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Write};
    use std::os::unix::net::UnixStream;

    use super::*;
    use crate::peer_token;
    use crate::testutil::{
        HARDENED, PROBE_ID, Signing, StdChild, TempDir, codesign_cdhash, expect_line, fd_lock,
        sh_writing_to, signed_probe, start_probe,
    };

    const PROBE: Requirement = Requirement::new("identifier \"dev.cupper.test.peer\"");
    const OTHER: Requirement = Requirement::new("identifier \"dev.cupper.test.other\"");
    /// Shaped like the requirements of release builds: ad-hoc code never
    /// satisfies it.
    const APPLE_ANCHOR: Requirement =
        Requirement::new("anchor apple generic and identifier \"dev.cupper.test.peer\"");

    /// A probe signed as `signing`, running on a socket whose other end is
    /// returned, with the probe's token.
    #[test]
    fn a_process_by_pid_names_its_executable_and_cdhash() {
        let _fds = fd_lock();
        let dir = TempDir::new();
        let probe = running(&dir, "by-pid", HARDENED);
        let pid = probe.child.0.id() as i32;
        let path = probe.path.canonicalize().unwrap();
        assert_eq!(crate::pid_path(pid).unwrap(), path);
        assert_eq!(pid_cdhash(pid).unwrap(), file_cdhash(&path).unwrap());
        assert!(crate::pid_path(i32::MAX).is_err());
        assert!(pid_cdhash(i32::MAX).is_err());
    }

    struct Running {
        ours: UnixStream,
        child: StdChild,
        path: std::path::PathBuf,
        token: AuditToken,
    }

    fn running(dir: &TempDir, name: &str, signing: Signing) -> Running {
        let path = signed_probe(dir.path(), name, signing);
        let (ours, theirs) = UnixStream::pair().unwrap();
        let child = start_probe(&path, theirs, &ours);
        let token = peer_token(&ours).unwrap();
        assert_eq!(token.pid(), child.0.id() as i32);
        Running {
            ours,
            child,
            path,
            token,
        }
    }

    #[test]
    fn it_sys_peer_current_verify_checks_requirement_and_status() {
        let _fds = fd_lock();
        let dir = TempDir::new();
        for requirement in [PROBE, OTHER, APPLE_ANCHOR] {
            requirement.validate().unwrap();
        }

        let good = running(&dir, "good", HARDENED);
        assert_eq!(good.token.verify(PROBE), Ok(()));
        for requirement in [OTHER, APPLE_ANCHOR] {
            let refusal = good.token.verify(requirement).unwrap_err();
            assert_eq!(
                (refusal.kind, refusal.code),
                (RefusalKind::Unsatisfied, ERR_SEC_CS_REQ_FAILED.into()),
                "{refusal}"
            );
        }

        let plain = Signing {
            runtime: false,
            ..HARDENED
        };
        let refusal = running(&dir, "plain", plain)
            .token
            .verify(PROBE)
            .unwrap_err();
        assert_eq!(refusal.kind, RefusalKind::NoRuntime, "{refusal}");

        let debuggable = Signing {
            get_task_allow: true,
            ..HARDENED
        };
        let refusal = running(&dir, "debuggable", debuggable)
            .token
            .verify(PROBE)
            .unwrap_err();
        assert_eq!(refusal.kind, RefusalKind::GetTaskAllow, "{refusal}");
        assert!(CodeStatus(refusal.code as u32).has(CodeStatus::GET_TASK_ALLOW));
    }

    #[test]
    fn signing_identity_of_an_ad_hoc_signed_peer() {
        let _fds = fd_lock();
        let dir = TempDir::new();
        let peer = running(&dir, "identity", HARDENED);
        let identity = peer.token.signing_identity().unwrap().unwrap();
        assert_eq!(
            (identity.identifier.as_str(), identity.team_id),
            (PROBE_ID, None)
        );
    }

    #[test]
    fn it_sys_peer_current_csops_reads_the_running_cdhash() {
        let _fds = fd_lock();
        let dir = TempDir::new();
        let peer = running(&dir, "peer", HARDENED);
        let cdhash = peer.token.cdhash().unwrap();
        assert_eq!(cdhash, file_cdhash(&peer.path).unwrap());
        assert_eq!(cdhash.to_string(), codesign_cdhash(&peer.path));
        let status = peer.token.code_status().unwrap();
        // The kernel does not report CS_ADHOC here (macOS 26.4: 0x22011311).
        assert!(
            status.has(CodeStatus::VALID | CodeStatus::RUNTIME),
            "{status:x?}"
        );
        assert!(!status.has(CodeStatus::GET_TASK_ALLOW), "{status:x?}");

        // The other build of the same program has another cdhash.
        let other = signed_probe(
            dir.path(),
            "other",
            Signing {
                identifier: "dev.cupper.test.other",
                ..HARDENED
            },
        );
        assert_ne!(file_cdhash(&other).unwrap(), cdhash);

        let exe = std::env::current_exe().unwrap();
        assert_eq!(own_cdhash().unwrap(), file_cdhash(&exe).unwrap());
    }

    #[test]
    fn it_sys_peer_current_changed_code_on_disk_is_refused() {
        let _fds = fd_lock();
        let dir = TempDir::new();

        // Removed, as a whole-bundle upgrade does with the old copy.
        let removed = running(&dir, "removed", HARDENED);
        assert_eq!(removed.token.verify(PROBE), Ok(()));
        std::fs::remove_file(&removed.path).unwrap();
        let refusal = removed.token.verify(PROBE).unwrap_err();
        assert_eq!(refusal.kind, RefusalKind::CodeChanged, "{refusal}");

        // Replaced by another build renamed over it, as an installer does.
        let replaced = running(&dir, "replaced", HARDENED);
        let running_cdhash = replaced.token.cdhash().unwrap();
        let new_build = signed_probe(
            dir.path(),
            "new-build",
            Signing {
                identifier: PROBE_ID,
                runtime: true,
                get_task_allow: true,
            },
        );
        std::fs::rename(&new_build, &replaced.path).unwrap();
        let refusal = replaced.token.verify(PROBE).unwrap_err();
        assert_eq!(
            (refusal.kind, refusal.code),
            (
                RefusalKind::CodeChanged,
                ERR_SEC_CS_STATIC_CODE_CHANGED.into()
            ),
            "{refusal}"
        );
        // The self-check signal: what runs is not what is on disk.
        assert_eq!(replaced.token.cdhash().unwrap(), running_cdhash);
        assert_ne!(file_cdhash(&replaced.path).unwrap(), running_cdhash);
    }

    #[test]
    fn it_sys_peer_current_exited_and_execed_peers_are_refused() {
        let _fds = fd_lock();
        let dir = TempDir::new();
        let Running {
            ours,
            mut child,
            token,
            ..
        } = running(&dir, "exits", HARDENED);
        assert_eq!(token.verify(PROBE), Ok(()));
        drop(ours);
        assert!(child.0.wait().unwrap().success());
        for refusal in [
            token.verify(PROBE).unwrap_err(),
            token.cdhash().unwrap_err(),
            token.code_status().unwrap_err(),
        ] {
            assert_eq!(refusal.kind, RefusalKind::PeerExited, "{refusal}");
        }

        let (server, client) = UnixStream::pair().unwrap();
        let mut shell = sh_writing_to(
            "echo before; read go; exec /bin/sh -c 'echo after; read go || true'",
            &client,
        );
        drop(client);
        let mut go = shell.0.stdin.take().unwrap();
        let mut lines = BufReader::new(&server);
        expect_line(&mut lines, "before");
        let before = peer_token(&server).unwrap();
        before.cdhash().unwrap();
        go.write_all(b"go\n").unwrap();
        expect_line(&mut lines, "after");
        for refusal in [
            before.verify(APPLE_ANCHOR).unwrap_err(),
            before.cdhash().unwrap_err(),
            before.code_status().unwrap_err(),
        ] {
            assert_eq!(refusal.kind, RefusalKind::PeerExeced, "{refusal}");
        }
        drop(go);
        assert!(shell.0.wait().unwrap().success());
    }

    #[test]
    fn it_sys_peer_current_malformed_requirement_is_other() {
        let malformed = Requirement::new("identifier");
        assert!(matches!(
            malformed.validate(),
            Err(Error::Security {
                call: "SecRequirementCreateWithString",
                ..
            })
        ));
        // Our own token, taken from a socket pair, is enough to get that far.
        let (server, client) = UnixStream::pair().unwrap();
        (&client).write_all(b"x").unwrap();
        crate::testutil::expect_byte(&server, b'x');
        let refusal = peer_token(&server).unwrap().verify(malformed).unwrap_err();
        assert_eq!(
            (refusal.kind, refusal.call),
            (RefusalKind::Other, "SecRequirementCreateWithString"),
            "{refusal}"
        );
        // A file that is not code.
        assert!(matches!(
            file_cdhash(Path::new("/etc/hosts")),
            Err(Error::Security { .. })
        ));
    }
}
