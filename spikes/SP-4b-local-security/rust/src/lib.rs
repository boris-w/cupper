//! SP-4b spike helpers: peer identity over Unix domain sockets, code-signing
//! checks via the Security framework, and process lineage via libproc.
//!
//! Spike code only. It favours explicit output over robustness.

use std::ffi::{CStr, CString, c_void};
use std::io;
use std::os::fd::RawFd;
use std::time::Instant;

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::error::{CFError, CFErrorRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::{CFURL, CFURLRef};
use core_foundation_sys::base::{CFRelease, CFTypeRef, OSStatus};
use core_foundation_sys::dictionary::CFDictionaryGetValue;
use serde_json::{Value, json};

pub type AuditToken = [u32; 8];

// ---------------------------------------------------------------------------
// FFI
// ---------------------------------------------------------------------------

pub mod ffi {
    use super::*;

    pub const K_SEC_CS_DEFAULT_FLAGS: u32 = 0;
    pub const K_SEC_CS_SIGNING_INFORMATION: u32 = 1 << 1;
    pub const K_SEC_CS_REQUIREMENT_INFORMATION: u32 = 1 << 2;
    pub const K_SEC_CS_DYNAMIC_INFORMATION: u32 = 1 << 3;
    pub const K_SEC_CS_DO_NOT_VALIDATE_RESOURCES: u32 = 1 << 2;
    pub const K_SEC_CS_STRICT_VALIDATE: u32 = 1 << 4;
    pub const K_SEC_CS_MATCH_GUEST_REQUIREMENT_IN_KERNEL: u32 = 1 << 23;
    pub const K_SEC_CS_NO_NETWORK_ACCESS: u32 = 1 << 29;

    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        pub static kSecGuestAttributeAudit: CFStringRef;
        pub static kSecGuestAttributePid: CFStringRef;
        pub static kSecCodeInfoIdentifier: CFStringRef;
        pub static kSecCodeInfoTeamIdentifier: CFStringRef;
        pub static kSecCodeInfoFlags: CFStringRef;
        pub static kSecCodeInfoStatus: CFStringRef;
        pub static kSecCodeInfoUnique: CFStringRef;
        pub static kSecCodeInfoEntitlementsDict: CFStringRef;
        pub static kSecCodeInfoMainExecutable: CFStringRef;

        pub fn SecCodeCopyGuestWithAttributes(
            host: *const c_void,
            attrs: CFDictionaryRef,
            flags: u32,
            guest: *mut *const c_void,
        ) -> OSStatus;
        pub fn SecCodeCheckValidityWithErrors(
            code: *const c_void,
            flags: u32,
            req: *const c_void,
            errors: *mut CFErrorRef,
        ) -> OSStatus;
        pub fn SecStaticCodeCheckValidityWithErrors(
            code: *const c_void,
            flags: u32,
            req: *const c_void,
            errors: *mut CFErrorRef,
        ) -> OSStatus;
        pub fn SecRequirementCreateWithString(
            text: CFStringRef,
            flags: u32,
            req: *mut *const c_void,
        ) -> OSStatus;
        pub fn SecCodeCopySigningInformation(
            code: *const c_void,
            flags: u32,
            info: *mut CFDictionaryRef,
        ) -> OSStatus;
        pub fn SecCodeCopySelf(flags: u32, out: *mut *const c_void) -> OSStatus;
        pub fn SecStaticCodeCreateWithPath(
            path: CFURLRef,
            flags: u32,
            out: *mut *const c_void,
        ) -> OSStatus;
        pub fn SecCodeCopyPath(code: *const c_void, flags: u32, path: *mut CFURLRef) -> OSStatus;
        pub fn SecCopyErrorMessageString(status: OSStatus, reserved: *mut c_void) -> CFStringRef;
    }

    // libsystem_kernel; declared in the private <sys/codesign.h>.
    unsafe extern "C" {
        pub fn csops(pid: i32, ops: u32, buf: *mut c_void, size: usize) -> i32;
        pub fn csops_audittoken(
            pid: i32,
            ops: u32,
            buf: *mut c_void,
            size: usize,
            token: *const AuditToken,
        ) -> i32;
    }
    pub const CS_OPS_STATUS: u32 = 0;
    pub const CS_OPS_CDHASH: u32 = 5;
}

// Dynamic code-signing status bits (kernel csflags, see <kern/cs_blobs.h>).
pub const CS_VALID: u32 = 0x1;
pub const CS_ADHOC: u32 = 0x2;
pub const CS_GET_TASK_ALLOW: u32 = 0x4;
pub const CS_HARD: u32 = 0x100;
pub const CS_KILL: u32 = 0x200;
pub const CS_RUNTIME: u32 = 0x10000;
pub const CS_LINKER_SIGNED: u32 = 0x20000;
pub const CS_PLATFORM_BINARY: u32 = 0x4000000;
pub const CS_DEBUGGED: u32 = 0x10000000;

pub fn csflags_names(f: u32) -> Vec<&'static str> {
    let table = [
        (CS_VALID, "valid"),
        (CS_ADHOC, "adhoc"),
        (CS_GET_TASK_ALLOW, "get-task-allow"),
        (CS_HARD, "hard"),
        (CS_KILL, "kill"),
        (CS_RUNTIME, "runtime"),
        (CS_LINKER_SIGNED, "linker-signed"),
        (CS_PLATFORM_BINARY, "platform"),
        (CS_DEBUGGED, "debugged"),
    ];
    table.iter().filter(|(b, _)| f & b != 0).map(|(_, n)| *n).collect()
}

/// Owned CoreFoundation reference released on drop.
pub struct Cf(pub *const c_void);
impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0 as CFTypeRef) }
        }
    }
}

pub fn status_message(st: OSStatus) -> String {
    unsafe {
        let s = ffi::SecCopyErrorMessageString(st, std::ptr::null_mut());
        if s.is_null() {
            return format!("OSStatus {st}");
        }
        CFString::wrap_under_create_rule(s).to_string()
    }
}

// ---------------------------------------------------------------------------
// Socket peer identity
// ---------------------------------------------------------------------------

const SOL_LOCAL: libc::c_int = 0;
const LOCAL_PEERCRED: libc::c_int = 0x001;
const LOCAL_PEERPID: libc::c_int = 0x002;
const LOCAL_PEEREPID: libc::c_int = 0x003;
const LOCAL_PEERTOKEN: libc::c_int = 0x006;

fn getsockopt_raw<T: Copy + Default>(fd: RawFd, opt: libc::c_int) -> io::Result<T> {
    let mut v = T::default();
    let mut len = std::mem::size_of::<T>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(fd, SOL_LOCAL, opt, &mut v as *mut T as *mut c_void, &mut len)
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(v)
}

pub fn peer_token(fd: RawFd) -> io::Result<AuditToken> {
    getsockopt_raw::<AuditToken>(fd, LOCAL_PEERTOKEN)
}
pub fn peer_pid(fd: RawFd) -> io::Result<i32> {
    getsockopt_raw::<i32>(fd, LOCAL_PEERPID)
}
pub fn peer_epid(fd: RawFd) -> io::Result<i32> {
    getsockopt_raw::<i32>(fd, LOCAL_PEEREPID)
}
pub fn peer_uid(fd: RawFd) -> io::Result<u32> {
    #[derive(Clone, Copy)]
    #[repr(C)]
    struct Xucred {
        version: u32,
        uid: u32,
        ngroups: i16,
        groups: [u32; 16],
    }
    impl Default for Xucred {
        fn default() -> Self {
            Xucred { version: 0, uid: u32::MAX, ngroups: 0, groups: [0; 16] }
        }
    }
    getsockopt_raw::<Xucred>(fd, LOCAL_PEERCRED).map(|c| c.uid)
}

/// audit_token_t layout: [auid, euid, egid, ruid, rgid, pid, asid, pidversion]
pub fn token_json(t: &AuditToken) -> Value {
    json!({"euid": t[1], "pid": t[5], "asid": t[6], "pidversion": t[7]})
}

pub fn self_token() -> io::Result<AuditToken> {
    // task_info(mach_task_self(), TASK_AUDIT_TOKEN)
    unsafe extern "C" {
        fn task_info(task: u32, flavor: i32, out: *mut i32, count: *mut u32) -> i32;
        static mach_task_self_: u32;
    }
    const TASK_AUDIT_TOKEN: i32 = 15;
    let mut tok: AuditToken = [0; 8];
    let mut count: u32 = 8;
    let kr = unsafe { task_info(mach_task_self_, TASK_AUDIT_TOKEN, tok.as_mut_ptr() as *mut i32, &mut count) };
    if kr != 0 {
        return Err(io::Error::other(format!("task_info kr={kr}")));
    }
    Ok(tok)
}

// ---------------------------------------------------------------------------
// Code objects
// ---------------------------------------------------------------------------

pub fn code_for_token(tok: &AuditToken) -> Result<Cf, OSStatus> {
    unsafe {
        let data = CFData::from_buffer(std::slice::from_raw_parts(tok.as_ptr() as *const u8, 32));
        let key = CFString::wrap_under_get_rule(ffi::kSecGuestAttributeAudit);
        let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), data.as_CFType())]);
        let mut code: *const c_void = std::ptr::null();
        let st = ffi::SecCodeCopyGuestWithAttributes(
            std::ptr::null(),
            dict.as_concrete_TypeRef(),
            0,
            &mut code,
        );
        if st != 0 { Err(st) } else { Ok(Cf(code)) }
    }
}

pub fn code_for_pid(pid: i32) -> Result<Cf, OSStatus> {
    unsafe {
        let num = CFNumber::from(pid);
        let key = CFString::wrap_under_get_rule(ffi::kSecGuestAttributePid);
        let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), num.as_CFType())]);
        let mut code: *const c_void = std::ptr::null();
        let st = ffi::SecCodeCopyGuestWithAttributes(
            std::ptr::null(),
            dict.as_concrete_TypeRef(),
            0,
            &mut code,
        );
        if st != 0 { Err(st) } else { Ok(Cf(code)) }
    }
}

pub fn code_self() -> Result<Cf, OSStatus> {
    let mut code: *const c_void = std::ptr::null();
    let st = unsafe { ffi::SecCodeCopySelf(0, &mut code) };
    if st != 0 { Err(st) } else { Ok(Cf(code)) }
}

pub fn static_code_for_path(path: &str) -> Result<Cf, OSStatus> {
    let url = CFURL::from_path(path, std::path::Path::new(path).is_dir()).ok_or(-50)?;
    let mut code: *const c_void = std::ptr::null();
    let st = unsafe { ffi::SecStaticCodeCreateWithPath(url.as_concrete_TypeRef(), 0, &mut code) };
    if st != 0 { Err(st) } else { Ok(Cf(code)) }
}

pub fn requirement(text: &str) -> Result<Cf, OSStatus> {
    let s = CFString::new(text);
    let mut req: *const c_void = std::ptr::null();
    let st = unsafe { ffi::SecRequirementCreateWithString(s.as_concrete_TypeRef(), 0, &mut req) };
    if st != 0 { Err(st) } else { Ok(Cf(req)) }
}

#[derive(Debug, Clone)]
pub struct CheckError {
    pub status: OSStatus,
    pub message: String,
}

impl CheckError {
    pub fn json(&self) -> Value {
        json!({"status": self.status, "message": self.message})
    }
}

fn cf_error_text(err: CFErrorRef) -> String {
    if err.is_null() {
        return String::new();
    }
    let e = unsafe { CFError::wrap_under_create_rule(err) };
    e.description().to_string()
}

/// SecCodeCheckValidityWithErrors on a dynamic code object.
pub fn check_code(code: &Cf, flags: u32, req: Option<&str>) -> Result<(), CheckError> {
    let r = match req {
        Some(t) => Some(requirement(t).map_err(|s| CheckError { status: s, message: status_message(s) })?),
        None => None,
    };
    let mut err: CFErrorRef = std::ptr::null_mut();
    let st = unsafe {
        ffi::SecCodeCheckValidityWithErrors(
            code.0,
            flags,
            r.as_ref().map(|r| r.0).unwrap_or(std::ptr::null()),
            &mut err,
        )
    };
    if st == 0 {
        Ok(())
    } else {
        let detail = cf_error_text(err);
        Err(CheckError { status: st, message: if detail.is_empty() { status_message(st) } else { detail } })
    }
}

/// SecStaticCodeCheckValidityWithErrors on an on-disk code object.
pub fn check_static(code: &Cf, flags: u32, req: Option<&str>) -> Result<(), CheckError> {
    let r = match req {
        Some(t) => Some(requirement(t).map_err(|s| CheckError { status: s, message: status_message(s) })?),
        None => None,
    };
    let mut err: CFErrorRef = std::ptr::null_mut();
    let st = unsafe {
        ffi::SecStaticCodeCheckValidityWithErrors(
            code.0,
            flags,
            r.as_ref().map(|r| r.0).unwrap_or(std::ptr::null()),
            &mut err,
        )
    };
    if st == 0 {
        Ok(())
    } else {
        let detail = cf_error_text(err);
        Err(CheckError { status: st, message: if detail.is_empty() { status_message(st) } else { detail } })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SigInfo {
    pub identifier: Option<String>,
    pub team_id: Option<String>,
    pub static_flags: Option<u32>,
    pub dynamic_status: Option<u32>,
    pub cdhash: Option<String>,
    pub ent_get_task_allow: bool,
    pub main_executable: Option<String>,
}

unsafe fn dict_get(dict: CFDictionaryRef, key: CFStringRef) -> *const c_void {
    unsafe { CFDictionaryGetValue(dict, key as *const c_void) }
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// SecCodeCopySigningInformation. `dynamic` must only be set for SecCode (not static) objects.
pub fn signing_info(code: &Cf, dynamic: bool) -> Result<SigInfo, OSStatus> {
    let mut flags = ffi::K_SEC_CS_SIGNING_INFORMATION | ffi::K_SEC_CS_REQUIREMENT_INFORMATION;
    if dynamic {
        flags |= ffi::K_SEC_CS_DYNAMIC_INFORMATION;
    }
    let mut info: CFDictionaryRef = std::ptr::null();
    let st = unsafe { ffi::SecCodeCopySigningInformation(code.0, flags, &mut info) };
    if st != 0 {
        return Err(st);
    }
    let _guard = Cf(info as *const c_void);
    let mut out = SigInfo::default();
    unsafe {
        let v = dict_get(info, ffi::kSecCodeInfoIdentifier);
        if !v.is_null() {
            out.identifier = Some(CFString::wrap_under_get_rule(v as CFStringRef).to_string());
        }
        let v = dict_get(info, ffi::kSecCodeInfoTeamIdentifier);
        if !v.is_null() {
            out.team_id = Some(CFString::wrap_under_get_rule(v as CFStringRef).to_string());
        }
        let v = dict_get(info, ffi::kSecCodeInfoFlags);
        if !v.is_null() {
            out.static_flags = CFNumber::wrap_under_get_rule(v as _).to_i64().map(|x| x as u32);
        }
        let v = dict_get(info, ffi::kSecCodeInfoStatus);
        if !v.is_null() {
            out.dynamic_status = CFNumber::wrap_under_get_rule(v as _).to_i64().map(|x| x as u32);
        }
        let v = dict_get(info, ffi::kSecCodeInfoUnique);
        if !v.is_null() {
            out.cdhash = Some(hex(CFData::wrap_under_get_rule(v as _).bytes()));
        }
        let v = dict_get(info, ffi::kSecCodeInfoEntitlementsDict);
        if !v.is_null() {
            let key = CFString::new("com.apple.security.get-task-allow");
            let b = CFDictionaryGetValue(v as CFDictionaryRef, key.as_concrete_TypeRef() as *const c_void);
            if !b.is_null() {
                out.ent_get_task_allow = CFBoolean::wrap_under_get_rule(b as _).into();
            }
        }
        let v = dict_get(info, ffi::kSecCodeInfoMainExecutable);
        if !v.is_null() {
            out.main_executable = CFURL::wrap_under_get_rule(v as CFURLRef)
                .to_path()
                .map(|p| p.display().to_string());
        }
    }
    Ok(out)
}

impl SigInfo {
    pub fn json(&self, redact_team: bool) -> Value {
        json!({
            "identifier": self.identifier,
            "team_id": if redact_team { self.team_id.as_ref().map(|_| "<TEAM>".to_string()) } else { self.team_id.clone() },
            "static_flags": self.static_flags.map(|f| format!("{f:#x}")),
            "dynamic_status": self.dynamic_status.map(|f| format!("{f:#x}")),
            "dynamic_status_names": self.dynamic_status.map(csflags_names),
            "cdhash": self.cdhash,
            "ent_get_task_allow": self.ent_get_task_allow,
        })
    }
}

/// Kernel view of a running process: csflags and cdhash, bound to an audit token.
pub fn kernel_cs(tok: &AuditToken) -> Value {
    let pid = tok[5] as i32;
    let mut status: u32 = 0;
    let rc1 = unsafe {
        ffi::csops_audittoken(pid, ffi::CS_OPS_STATUS, &mut status as *mut u32 as *mut c_void, 4, tok)
    };
    let e1 = if rc1 != 0 { Some(io::Error::last_os_error().to_string()) } else { None };
    let mut cd = [0u8; 20];
    let rc2 = unsafe { ffi::csops_audittoken(pid, ffi::CS_OPS_CDHASH, cd.as_mut_ptr() as *mut c_void, 20, tok) };
    let e2 = if rc2 != 0 { Some(io::Error::last_os_error().to_string()) } else { None };
    json!({
        "csops_status": if rc1 == 0 { Some(format!("{status:#x}")) } else { None },
        "csops_status_err": e1,
        "csops_cdhash": if rc2 == 0 { Some(hex(&cd)) } else { None },
        "csops_cdhash_err": e2,
    })
}

// ---------------------------------------------------------------------------
// Verification policy (what the host would do for the `desktop_ui` principal)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Policy {
    pub requirement: Option<String>,
    pub check_flags: u32,
    pub require_runtime: bool,
    pub forbid_get_task_allow: bool,
}

impl Policy {
    pub fn json(&self) -> Value {
        json!({
            "requirement": self.requirement.as_deref().map(redact_team_in),
            "check_flags": format!("{:#x}", self.check_flags),
            "require_runtime": self.require_runtime,
            "forbid_get_task_allow": self.forbid_get_task_allow,
        })
    }
}

/// Replace any 10-char Team ID literal in a requirement with a placeholder.
pub fn redact_team_in(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("subject.OU] = ") {
        let (head, tail) = rest.split_at(i + "subject.OU] = ".len());
        out.push_str(head);
        let tail = tail.trim_start_matches('"');
        let end = tail.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(tail.len());
        out.push_str("\"<TEAM>");
        rest = &tail[end..];
        if !rest.starts_with('"') {
            out.push('"');
        }
    }
    out.push_str(rest);
    out
}

pub struct Verdict {
    pub accept: bool,
    pub stage: &'static str,
    pub detail: Value,
}

/// Full check: audit token -> SecCode -> requirement -> dynamic flags.
pub fn verify_token(tok: &AuditToken, policy: &Policy) -> Verdict {
    let t0 = Instant::now();
    let code = match code_for_token(tok) {
        Ok(c) => c,
        Err(st) => {
            return Verdict {
                accept: false,
                stage: "copy_guest",
                detail: json!({"status": st, "message": status_message(st), "elapsed_us": t0.elapsed().as_micros() as u64}),
            };
        }
    };
    let t_copy = t0.elapsed().as_micros() as u64;
    let check = check_code(&code, policy.check_flags, policy.requirement.as_deref());
    let t_check = t0.elapsed().as_micros() as u64;
    let info = signing_info(&code, true).ok();
    let info_json = info.as_ref().map(|i| i.json(true));
    let kern = kernel_cs(tok);
    if let Err(e) = check {
        return Verdict {
            accept: false,
            stage: "check_validity",
            detail: json!({"error": e.json(), "info": info_json, "kernel": kern, "copy_us": t_copy, "check_us": t_check}),
        };
    }
    let status = info.as_ref().and_then(|i| i.dynamic_status).unwrap_or(0);
    let mut reasons = vec![];
    if policy.require_runtime && status & CS_RUNTIME == 0 {
        reasons.push("hardened runtime not enabled");
    }
    if policy.forbid_get_task_allow && status & CS_GET_TASK_ALLOW != 0 {
        reasons.push("get-task-allow present");
    }
    if policy.forbid_get_task_allow && status & CS_DEBUGGED != 0 {
        reasons.push("process has been debugged");
    }
    Verdict {
        accept: reasons.is_empty(),
        stage: if reasons.is_empty() { "accepted" } else { "flags" },
        detail: json!({"reasons": reasons, "info": info_json, "kernel": kern, "copy_us": t_copy, "check_us": t_check}),
    }
}

// ---------------------------------------------------------------------------
// Process info / lineage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ProcInfo {
    pub pid: i32,
    pub ppid: i32,
    pub pgid: i32,
    pub sid: i32,
    pub start: (u64, u64),
    pub comm: String,
}

pub fn proc_info(pid: i32) -> Option<ProcInfo> {
    let mut bi: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let n = unsafe {
        libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, &mut bi as *mut _ as *mut c_void, size)
    };
    if n != size {
        return None;
    }
    let comm = unsafe { CStr::from_ptr(bi.pbi_comm.as_ptr()) }.to_string_lossy().into_owned();
    Some(ProcInfo {
        pid,
        ppid: bi.pbi_ppid as i32,
        pgid: bi.pbi_pgid as i32,
        sid: unsafe { libc::getsid(pid) },
        start: (bi.pbi_start_tvsec, bi.pbi_start_tvusec),
        comm,
    })
}

pub fn proc_path(pid: i32) -> Option<String> {
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) };
    if n <= 0 {
        return None;
    }
    buf.truncate(n as usize);
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Ancestor chain starting at `pid` (inclusive), stopping at launchd.
pub fn ancestors(pid: i32) -> Vec<ProcInfo> {
    let mut out = vec![];
    let mut cur = pid;
    for _ in 0..64 {
        match proc_info(cur) {
            Some(p) => {
                let next = p.ppid;
                out.push(p);
                if cur <= 1 || next == cur {
                    break;
                }
                cur = next;
            }
            None => break,
        }
    }
    out
}

/// Private SPI used by TCC to attribute child processes; looked up dynamically.
pub fn responsible_pid(pid: i32) -> Option<i32> {
    type F = unsafe extern "C" fn(i32) -> i32;
    let name = CString::new("responsibility_get_pid_responsible_for_pid").unwrap();
    let sym = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
    if sym.is_null() {
        return None;
    }
    let f: F = unsafe { std::mem::transmute(sym) };
    let r = unsafe { f(pid) };
    if r <= 0 { None } else { Some(r) }
}

// ---------------------------------------------------------------------------
// Small CLI helpers
// ---------------------------------------------------------------------------

/// Options are only parsed before a literal `--`; everything after it is `rest()`.
pub struct Args(pub Vec<String>, Vec<String>);
impl Args {
    pub fn from_env() -> Self {
        let all: Vec<String> = std::env::args().skip(1).collect();
        match all.iter().position(|a| a == "--") {
            Some(i) => Args(all[..i].to_vec(), all[i + 1..].to_vec()),
            None => Args(all, vec![]),
        }
    }
    pub fn flag(&self, name: &str) -> bool {
        self.0.iter().any(|a| a == name)
    }
    pub fn opt(&self, name: &str) -> Option<String> {
        self.0.iter().position(|a| a == name).and_then(|i| self.0.get(i + 1).cloned())
    }
    pub fn opts(&self, name: &str) -> Vec<String> {
        let mut v = vec![];
        let mut i = 0;
        while i < self.0.len() {
            if self.0[i] == name {
                if let Some(x) = self.0.get(i + 1) {
                    v.push(x.clone());
                }
                i += 2;
            } else {
                i += 1;
            }
        }
        v
    }
    /// Everything after a literal `--`.
    pub fn rest(&self) -> Vec<String> {
        self.1.clone()
    }
}

pub fn parse_flags(s: &str) -> u32 {
    let mut f = 0;
    for part in s.split(',').filter(|p| !p.is_empty()) {
        f |= match part {
            "default" => 0,
            "kernel" => ffi::K_SEC_CS_MATCH_GUEST_REQUIREMENT_IN_KERNEL,
            "strict" => ffi::K_SEC_CS_STRICT_VALIDATE,
            "nores" => ffi::K_SEC_CS_DO_NOT_VALIDATE_RESOURCES,
            "nonet" => ffi::K_SEC_CS_NO_NETWORK_ACCESS,
            other => u32::from_str_radix(other.trim_start_matches("0x"), 16).expect("bad flag"),
        };
    }
    f
}
