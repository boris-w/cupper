//! Thin wrappers over macOS system calls. All `unsafe` code of the host lives here.

use std::ffi::{CString, c_char, c_int};
use std::io;
use std::os::fd::{FromRawFd, OwnedFd};
use std::path::Path;
use std::ptr;

/// A freshly spawned adapter: its pid (also its process group id) and our pipe ends.
#[derive(Debug)]
pub struct Spawned {
    pub pid: i32,
    pub start_us: u64,
    /// Write end connected to the child's stdin.
    pub stdin: OwnedFd,
    /// Read end connected to the child's stdout.
    pub stdout: OwnedFd,
}

fn check(rc: c_int) -> io::Result<()> {
    if rc == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(rc)) }
}

fn cstring(s: &str) -> io::Result<CString> {
    CString::new(s).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL byte in argument"))
}

fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0 as c_int; 2];
    // SAFETY: `fds` is a valid 2-element buffer.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    for fd in fds {
        // SAFETY: fd was just returned by pipe(2).
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    // SAFETY: both fds are open and owned by nobody else.
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

/// Spawn `program` with `posix_spawn`:
/// - in a new process group (`POSIX_SPAWN_SETPGROUP`, pgid = pid),
/// - with `POSIX_SPAWN_CLOEXEC_DEFAULT`, so only fds 0/1/2 set up here are inherited,
/// - with default signal dispositions and an empty signal mask,
/// - with exactly the environment given (the caller applies the allow-list),
/// - stdin/stdout as pipes and stderr appended to `stderr_log`.
pub fn spawn_adapter(program: &Path, args: &[String], env: &[(String, String)], stderr_log: &Path) -> io::Result<Spawned> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let prog = CString::new(program.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL byte in program path"))?;
    let log = CString::new(stderr_log.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL byte in log path"))?;
    let mut argv_c = vec![prog.clone()];
    for a in args {
        argv_c.push(cstring(a)?);
    }
    let envp_c = env.iter().map(|(k, v)| cstring(&format!("{k}={v}"))).collect::<io::Result<Vec<_>>>()?;
    let mut argv: Vec<*mut c_char> = argv_c.iter().map(|s| s.as_ptr() as *mut c_char).collect();
    argv.push(ptr::null_mut());
    let mut envp: Vec<*mut c_char> = envp_c.iter().map(|s| s.as_ptr() as *mut c_char).collect();
    envp.push(ptr::null_mut());

    let (child_stdin, our_stdin) = pipe()?;
    let (our_stdout, child_stdout) = pipe()?;

    // SAFETY: attr/actions are initialised before use and destroyed on every path;
    // all pointers passed to posix_spawn outlive the call.
    unsafe {
        let mut actions: libc::posix_spawn_file_actions_t = ptr::null_mut();
        check(libc::posix_spawn_file_actions_init(&mut actions))?;
        let mut attr: libc::posix_spawnattr_t = ptr::null_mut();
        if let Err(e) = check(libc::posix_spawnattr_init(&mut attr)) {
            libc::posix_spawn_file_actions_destroy(&mut actions);
            return Err(e);
        }
        let result: io::Result<libc::pid_t> = (|| {
            check(libc::posix_spawn_file_actions_adddup2(&mut actions, child_stdin.as_raw_fd(), 0))?;
            check(libc::posix_spawn_file_actions_adddup2(&mut actions, child_stdout.as_raw_fd(), 1))?;
            check(libc::posix_spawn_file_actions_addopen(
                &mut actions,
                2,
                log.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND,
                0o600,
            ))?;
            let flags = libc::POSIX_SPAWN_SETPGROUP
                | libc::POSIX_SPAWN_CLOEXEC_DEFAULT
                | libc::POSIX_SPAWN_SETSIGDEF
                | libc::POSIX_SPAWN_SETSIGMASK;
            check(libc::posix_spawnattr_setflags(&mut attr, flags as libc::c_short))?;
            check(libc::posix_spawnattr_setpgroup(&mut attr, 0))?;
            let mut all: libc::sigset_t = 0;
            libc::sigfillset(&mut all);
            check(libc::posix_spawnattr_setsigdefault(&mut attr, &all))?;
            let mut none: libc::sigset_t = 0;
            libc::sigemptyset(&mut none);
            check(libc::posix_spawnattr_setsigmask(&mut attr, &none))?;
            let mut pid: libc::pid_t = 0;
            check(libc::posix_spawn(&mut pid, prog.as_ptr(), &actions, &attr, argv.as_ptr(), envp.as_ptr()))?;
            Ok(pid)
        })();
        libc::posix_spawnattr_destroy(&mut attr);
        libc::posix_spawn_file_actions_destroy(&mut actions);
        let pid = result?;
        drop(child_stdin);
        drop(child_stdout);
        Ok(Spawned { pid, start_us: process_start_us(pid).unwrap_or(0), stdin: our_stdin, stdout: our_stdout })
    }
}

/// Block until `pid` exits; returns the raw wait status.
pub fn wait_blocking(pid: i32) -> io::Result<i32> {
    let mut status: c_int = 0;
    loop {
        // SAFETY: plain waitpid on a child we spawned.
        let rc = unsafe { libc::waitpid(pid, &mut status, 0) };
        if rc == pid {
            return Ok(status);
        }
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::Interrupted {
            return Err(err);
        }
    }
}

/// Send `signal` to every process in group `pgid`.
pub fn kill_group(pgid: i32, signal: c_int) -> io::Result<()> {
    // SAFETY: killpg has no memory-safety preconditions.
    if unsafe { libc::killpg(pgid, signal) } == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

fn bsd_info(pid: i32) -> Option<libc::proc_bsdinfo> {
    // SAFETY: proc_bsdinfo is plain data; proc_pidinfo writes at most `size` bytes.
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as c_int;
        let n = libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&mut info as *mut libc::proc_bsdinfo).cast(), size);
        (n == size).then_some(info)
    }
}

/// Process start time in microseconds since the epoch, used to tell a recorded
/// adapter apart from an unrelated process that reused its pid.
pub fn process_start_us(pid: i32) -> Option<u64> {
    bsd_info(pid).map(|i| i.pbi_start_tvsec * 1_000_000 + i.pbi_start_tvusec)
}

/// Live members of process group `pgid` with their start times.
pub fn group_members(pgid: i32) -> Vec<(i32, u64)> {
    let mut pids = vec![0 as c_int; 4096];
    let bytes = (pids.len() * std::mem::size_of::<c_int>()) as c_int;
    // SAFETY: the buffer holds `bytes` bytes.
    let n = unsafe { libc::proc_listpgrppids(pgid, pids.as_mut_ptr().cast(), bytes) };
    if n <= 0 {
        return Vec::new();
    }
    pids.truncate((n as usize).min(pids.len()));
    pids.into_iter()
        .filter(|&p| p > 0)
        .filter_map(|p| bsd_info(p).filter(|i| i.pbi_pgid as i32 == pgid).map(|i| (p, i.pbi_start_tvsec * 1_000_000 + i.pbi_start_tvusec)))
        .collect()
}

pub fn current_uid() -> u32 {
    // SAFETY: getuid cannot fail.
    unsafe { libc::getuid() }
}
