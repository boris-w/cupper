//! Starting child processes (design 5.8.5). Every child of the host, the CLI
//! and the native extension is started by [`spawn`]:
//!
//! - `POSIX_SPAWN_CLOEXEC_DEFAULT`: the child inherits fds 0, 1 and 2 as set
//!   up here and nothing else, whatever the parent has open;
//! - default dispositions for every signal and an empty signal mask;
//! - its own process group, or with [`Command::new_session`] its own session
//!   (`POSIX_SPAWN_SETSID`); either way the child leads its group, pgid = pid;
//! - exactly the environment of its [`Env`];
//! - an absolute program path; there is no `PATH` search.

use std::ffi::{CStr, CString, OsStr, OsString};
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::ptr;

use libc::{c_char, c_int};

use crate::proc::{StartTime, start_time};
use crate::{Env, Error, Result};

/// `POSIX_SPAWN_SETSID` from `<spawn.h>` (macOS 10.15 and later); the libc
/// crate does not define it.
const POSIX_SPAWN_SETSID: c_int = 0x0400;

/// What a child's stdin, stdout or stderr is connected to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stdio {
    /// `/dev/null`.
    Null,
    /// A pipe; the parent's end is in [`Child`].
    Pipe,
}

/// A child to start with [`spawn`].
#[derive(Clone, Debug)]
pub struct Command {
    program: PathBuf,
    args: Vec<OsString>,
    env: Env,
    cwd: Option<PathBuf>,
    stdio: [Stdio; 3],
    new_session: bool,
    /// Mutation check of UNIT-sys-spawn: drops `POSIX_SPAWN_CLOEXEC_DEFAULT`
    /// so the test can show that it would notice the leak.
    #[cfg(test)]
    pub(crate) without_cloexec_default: bool,
}

impl Command {
    /// A child running `program`, which must be an absolute path. Until set
    /// otherwise it has no arguments, an empty environment, stdio on
    /// `/dev/null`, our working directory and its own process group.
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Env::new(),
            cwd: None,
            stdio: [Stdio::Null; 3],
            new_session: false,
            #[cfg(test)]
            without_cloexec_default: false,
        }
    }

    pub fn arg(&mut self, arg: impl Into<OsString>) -> &mut Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(&mut self, env: Env) -> &mut Self {
        self.env = env;
        self
    }

    pub fn current_dir(&mut self, dir: impl Into<PathBuf>) -> &mut Self {
        self.cwd = Some(dir.into());
        self
    }

    pub fn stdin(&mut self, stdio: Stdio) -> &mut Self {
        self.stdio[0] = stdio;
        self
    }

    pub fn stdout(&mut self, stdio: Stdio) -> &mut Self {
        self.stdio[1] = stdio;
        self
    }

    pub fn stderr(&mut self, stdio: Stdio) -> &mut Self {
        self.stdio[2] = stdio;
        self
    }

    /// Start the child in a session of its own (`POSIX_SPAWN_SETSID`), as
    /// adapters and the host are (design 5.8.5), instead of only in a process
    /// group of its own.
    pub fn new_session(&mut self) -> &mut Self {
        self.new_session = true;
        self
    }
}

/// A started child. It must be waited for ([`Child::wait`]) or it stays a
/// zombie until we exit.
#[derive(Debug)]
pub struct Child {
    pid: i32,
    start: StartTime,
    status: Option<ExitStatus>,
    /// Our end of the child's stdin, when that is a pipe.
    pub stdin: Option<OwnedFd>,
    /// Our end of the child's stdout, when that is a pipe.
    pub stdout: Option<OwnedFd>,
    /// Our end of the child's stderr, when that is a pipe.
    pub stderr: Option<OwnedFd>,
}

impl Child {
    pub fn pid(&self) -> i32 {
        self.pid
    }

    /// When the child started; with the pid it names the child in records
    /// that outlive this handle (design 5.2.13).
    pub fn start(&self) -> StartTime {
        self.start
    }

    /// Blocks until the child exits.
    pub fn wait(&mut self) -> Result<ExitStatus> {
        loop {
            if let Some(status) = self.reap(0)? {
                return Ok(status);
            }
        }
    }

    /// The exit status if the child has exited, without blocking.
    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        self.reap(libc::WNOHANG)
    }

    fn reap(&mut self, flags: c_int) -> Result<Option<ExitStatus>> {
        if self.status.is_some() {
            return Ok(self.status);
        }
        let mut raw: c_int = 0;
        loop {
            // SAFETY: `raw` is a valid out pointer; `pid` is our own child,
            // which only this handle waits for.
            let rc = unsafe { libc::waitpid(self.pid, &mut raw, flags) };
            if rc == self.pid {
                self.status = Some(ExitStatus::from_raw(raw));
                return Ok(self.status);
            }
            if rc == 0 {
                return Ok(None);
            }
            let source = io::Error::last_os_error();
            if source.kind() != io::ErrorKind::Interrupted {
                return Err(Error::Os {
                    call: "waitpid",
                    source,
                });
            }
        }
    }
}

/// Starts `cmd` as described in the module documentation.
pub fn spawn(cmd: &Command) -> Result<Child> {
    if !cmd.program.is_absolute() {
        return Err(Error::InvalidArgument("program path must be absolute"));
    }
    let program = cstring(cmd.program.as_os_str())?;
    let mut arg_strings = vec![program.clone()];
    for arg in &cmd.args {
        arg_strings.push(cstring(arg)?);
    }
    let env_strings = cmd.env.to_cstrings();
    let cwd = match &cmd.cwd {
        Some(dir) => Some(cstring(dir.as_os_str())?),
        None => None,
    };

    // Our ends and the child's ends of the pipes. Both are close-on-exec; the
    // child's end only reaches the child through a dup2 file action.
    let mut ours: [Option<OwnedFd>; 3] = [None, None, None];
    let mut theirs: [Option<OwnedFd>; 3] = [None, None, None];
    for (fd, stdio) in cmd.stdio.iter().enumerate() {
        if *stdio == Stdio::Pipe {
            let (read, write) = io::pipe().map_err(|source| Error::Os {
                call: "pipe",
                source,
            })?;
            let (parent, child) = if fd == 0 {
                (OwnedFd::from(write), OwnedFd::from(read))
            } else {
                (OwnedFd::from(read), OwnedFd::from(write))
            };
            ours[fd] = Some(parent);
            theirs[fd] = Some(above_stdio(child)?);
        }
    }

    let mut actions = FileActions::new()?;
    for (fd, end) in (0..).zip(&theirs) {
        match end {
            Some(end) => actions.dup2(end.as_raw_fd(), fd)?,
            None => {
                let mode = if fd == 0 {
                    libc::O_RDONLY
                } else {
                    libc::O_WRONLY
                };
                actions.open(fd, c"/dev/null", mode)?;
            }
        }
    }
    if let Some(dir) = &cwd {
        actions.chdir(dir)?;
    }

    let mut flags = libc::POSIX_SPAWN_CLOEXEC_DEFAULT
        | libc::POSIX_SPAWN_SETSIGDEF
        | libc::POSIX_SPAWN_SETSIGMASK;
    flags |= if cmd.new_session {
        POSIX_SPAWN_SETSID
    } else {
        libc::POSIX_SPAWN_SETPGROUP
    };
    #[cfg(test)]
    if cmd.without_cloexec_default {
        flags &= !libc::POSIX_SPAWN_CLOEXEC_DEFAULT;
    }
    let attr = SpawnAttr::new(flags)?;

    let argv = null_terminated(&arg_strings);
    let envp = null_terminated(&env_strings);
    let mut pid: libc::pid_t = 0;
    // SAFETY: `program`, every `argv` and `envp` entry and the file actions'
    // paths are NUL-terminated strings that outlive the call; both arrays end
    // with a null pointer; `actions` and `attr` are initialised. posix_spawn
    // does not write through the argv or envp pointers.
    let rc = unsafe {
        libc::posix_spawn(
            &mut pid,
            program.as_ptr(),
            &actions.0,
            &attr.0,
            argv.as_ptr(),
            envp.as_ptr(),
        )
    };
    if rc != 0 {
        return Err(Error::Os {
            call: "posix_spawn",
            source: io::Error::from_raw_os_error(rc),
        });
    }
    drop(theirs);

    // A child we have not waited for is always in the process table, so this
    // does not fail; if it did, a child we cannot name must not live on.
    let Some(start) = start_time(pid) else {
        let mut raw: c_int = 0;
        // SAFETY: plain integers and a valid out pointer; `pid` is our
        // unreaped child.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
            libc::waitpid(pid, &mut raw, 0);
        }
        return Err(Error::Unexpected("start time of a new child"));
    };
    let [stdin, stdout, stderr] = ours;
    Ok(Child {
        pid,
        start,
        status: None,
        stdin,
        stdout,
        stderr,
    })
}

fn cstring(s: &OsStr) -> Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| Error::InvalidArgument("NUL byte in a path or argument"))
}

fn null_terminated(strings: &[CString]) -> Vec<*mut c_char> {
    strings
        .iter()
        .map(|s| s.as_ptr().cast_mut())
        .chain([ptr::null_mut()])
        .collect()
}

/// `fd`, moved above 2 if it is a standard fd. A dup2 file action whose
/// source is itself a stdio slot would be skipped (same fd) or clobbered by an
/// earlier action; this only happens when the parent has closed 0–2.
fn above_stdio(fd: OwnedFd) -> Result<OwnedFd> {
    if fd.as_raw_fd() > 2 {
        return Ok(fd);
    }
    // try_clone uses F_DUPFD_CLOEXEC with a minimum of 3.
    fd.try_clone().map_err(|source| Error::Os {
        call: "fcntl(F_DUPFD_CLOEXEC)",
        source,
    })
}

fn check(call: &'static str, rc: c_int) -> Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(Error::Os {
            call,
            source: io::Error::from_raw_os_error(rc),
        })
    }
}

/// An initialised `posix_spawn_file_actions_t`, destroyed on drop.
struct FileActions(libc::posix_spawn_file_actions_t);

impl FileActions {
    fn new() -> Result<Self> {
        let mut raw: libc::posix_spawn_file_actions_t = ptr::null_mut();
        // SAFETY: `raw` is a valid out pointer for the new object.
        check("posix_spawn_file_actions_init", unsafe {
            libc::posix_spawn_file_actions_init(&mut raw)
        })?;
        Ok(Self(raw))
    }

    fn dup2(&mut self, from: c_int, to: c_int) -> Result<()> {
        // SAFETY: `self.0` is initialised; the call only records the fds.
        check("posix_spawn_file_actions_adddup2", unsafe {
            libc::posix_spawn_file_actions_adddup2(&mut self.0, from, to)
        })
    }

    fn open(&mut self, fd: c_int, path: &'static CStr, mode: c_int) -> Result<()> {
        // SAFETY: `self.0` is initialised; `path` is NUL-terminated and
        // 'static, so it outlives the posix_spawn call that reads it.
        check("posix_spawn_file_actions_addopen", unsafe {
            libc::posix_spawn_file_actions_addopen(&mut self.0, fd, path.as_ptr(), mode, 0)
        })
    }

    fn chdir(&mut self, dir: &CStr) -> Result<()> {
        // SAFETY: `self.0` is initialised; `dir` is NUL-terminated and is
        // copied into the file actions object.
        check("posix_spawn_file_actions_addchdir_np", unsafe {
            libc::posix_spawn_file_actions_addchdir_np(&mut self.0, dir.as_ptr())
        })
    }
}

impl Drop for FileActions {
    fn drop(&mut self) {
        // SAFETY: `self.0` was initialised in `new` and is destroyed once.
        unsafe { libc::posix_spawn_file_actions_destroy(&mut self.0) };
    }
}

/// An initialised `posix_spawnattr_t` with the given flags, a fresh process
/// group (when not starting a session), every signal reset to its default
/// action and an empty signal mask. Destroyed on drop.
struct SpawnAttr(libc::posix_spawnattr_t);

impl SpawnAttr {
    fn new(flags: c_int) -> Result<Self> {
        let mut raw: libc::posix_spawnattr_t = ptr::null_mut();
        // SAFETY: `raw` is a valid out pointer for the new object.
        check("posix_spawnattr_init", unsafe {
            libc::posix_spawnattr_init(&mut raw)
        })?;
        let mut attr = Self(raw);
        let flags = libc::c_short::try_from(flags)
            .map_err(|_| Error::InvalidArgument("posix_spawn flags"))?;
        // SAFETY: `attr.0` is initialised; the call stores the flags.
        check("posix_spawnattr_setflags", unsafe {
            libc::posix_spawnattr_setflags(&mut attr.0, flags)
        })?;
        // SAFETY: as above; pgroup 0 means "a new group led by the child" and
        // only takes effect with POSIX_SPAWN_SETPGROUP.
        check("posix_spawnattr_setpgroup", unsafe {
            libc::posix_spawnattr_setpgroup(&mut attr.0, 0)
        })?;
        let mut all: libc::sigset_t = 0;
        let mut none: libc::sigset_t = 0;
        // SAFETY: both are valid sigset_t out pointers.
        unsafe {
            libc::sigfillset(&mut all);
            libc::sigemptyset(&mut none);
        }
        // SAFETY: `attr.0` is initialised and the sets are valid; the calls
        // copy them.
        check("posix_spawnattr_setsigdefault", unsafe {
            libc::posix_spawnattr_setsigdefault(&mut attr.0, &all)
        })?;
        // SAFETY: as above.
        check("posix_spawnattr_setsigmask", unsafe {
            libc::posix_spawnattr_setsigmask(&mut attr.0, &none)
        })?;
        Ok(attr)
    }
}

impl Drop for SpawnAttr {
    fn drop(&mut self) {
        // SAFETY: `self.0` was initialised in `new` and is destroyed once.
        unsafe { libc::posix_spawnattr_destroy(&mut self.0) };
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::process::ExitStatusExt;

    use super::*;
    use crate::process_info;
    use crate::testutil::{Owned, cat, inheritable_fd, open_fds, sh};

    /// Sends a line through `cat` and reads it back: once it returns, `cat`
    /// has finished starting and its fd table is final.
    fn echo(child: &mut Owned) -> std::fs::File {
        let mut stdin = child.stdin_file();
        let mut stdout = child.stdout_file();
        stdin.write_all(b"ping\n").unwrap();
        let mut line = [0u8; 5];
        stdout.read_exact(&mut line).unwrap();
        assert_eq!(&line, b"ping\n");
        stdin
    }

    #[test]
    fn unit_sys_spawn_child_inherits_only_fds_0_to_2() {
        let leak = inheritable_fd();
        let mut child = Owned::spawn(&cat());
        let stdin = echo(&mut child);
        assert_eq!(
            open_fds(child.pid()),
            [0, 1, 2],
            "parent also had fd {}",
            leak.as_raw_fd()
        );
        drop(stdin);
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn unit_sys_spawn_mutation_without_cloexec_default_leaks_fds() {
        // The same check as above, against a spawn without
        // POSIX_SPAWN_CLOEXEC_DEFAULT: it must see the leak, or it proves nothing.
        let leak = inheritable_fd();
        let mut cmd = cat();
        cmd.without_cloexec_default = true;
        let mut child = Owned::spawn(&cmd);
        let stdin = echo(&mut child);
        let fds = open_fds(child.pid());
        assert!(
            fds.contains(&leak.as_raw_fd()),
            "fd {} not leaked: {fds:?}",
            leak.as_raw_fd()
        );
        assert_ne!(fds, [0, 1, 2]);
        drop(stdin);
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn unit_sys_spawn_new_session_or_new_group() {
        let me = process_info(std::process::id() as i32).unwrap();

        let session = Owned::spawn(cat().new_session());
        let info = process_info(session.pid()).unwrap();
        assert_eq!((info.sid, info.pgid), (session.pid(), session.pid()));
        assert_ne!(info.sid, me.sid);
        assert_eq!(info.start, session.start());

        let group = Owned::spawn(&cat());
        let info = process_info(group.pid()).unwrap();
        assert_eq!((info.sid, info.pgid), (me.sid, group.pid()));
        assert_ne!(info.pgid, me.pgid);
    }

    #[test]
    fn unit_sys_spawn_child_gets_exactly_its_env() {
        let mut env = Env::from_allowed(std::env::vars_os(), &["HOME", "PATH"]);
        env.set("SYS_SPAWN_MARK", "a b=c").unwrap();
        let ours: Vec<_> = std::env::vars_os().map(|(k, _)| k).collect();
        assert!(
            ours.iter().any(|k| k != "HOME" && k != "PATH"),
            "the test needs another variable"
        );

        let mut cmd = Command::new("/usr/bin/env");
        cmd.env(env.clone()).stdout(Stdio::Pipe);
        let mut child = Owned::spawn(&cmd);
        let mut out = String::new();
        child.stdout_file().read_to_string(&mut out).unwrap();
        assert!(child.wait().unwrap().success());

        let got: Vec<&str> = out.lines().collect();
        let want: Vec<String> = env
            .iter()
            .map(|(k, v)| format!("{}={}", k.display(), v.display()))
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn unit_sys_spawn_resets_the_signal_mask() {
        // SIGTERM blocked in this thread would be inherited without
        // POSIX_SPAWN_SETSIGMASK, and `sleep` would then outlive it.
        let mut term: libc::sigset_t = 0;
        let mut old: libc::sigset_t = 0;
        // SAFETY: valid sigset_t pointers; the mask only affects this thread
        // and is restored right after the spawn.
        unsafe {
            libc::sigemptyset(&mut term);
            libc::sigaddset(&mut term, libc::SIGTERM);
            libc::pthread_sigmask(libc::SIG_BLOCK, &term, &mut old);
        }
        let mut cmd = Command::new("/bin/sleep");
        cmd.arg("10");
        let spawned = spawn(&cmd);
        // SAFETY: restores the mask saved above.
        unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &old, ptr::null_mut()) };
        let mut child = Owned(spawned.unwrap());
        // SAFETY: kill takes plain integers; the pid is our unreaped child.
        assert_eq!(unsafe { libc::kill(child.pid(), libc::SIGTERM) }, 0);
        assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGTERM));
    }

    #[test]
    fn unit_sys_spawn_null_stdin_and_working_directory() {
        let mut cmd = sh("pwd; read line || echo eof");
        cmd.current_dir("/usr");
        let mut child = Owned::spawn(&cmd);
        let mut out = String::new();
        child.stdout_file().read_to_string(&mut out).unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(out, "/usr\neof\n");
    }

    #[test]
    fn unit_sys_spawn_refuses_bad_programs_and_arguments() {
        assert!(matches!(
            spawn(&Command::new("sh")),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            spawn(Command::new("/bin/sh").arg("a\0b")),
            Err(Error::InvalidArgument(_))
        ));
        let missing = spawn(&Command::new("/nonexistent/cupper-sys-test"));
        match missing {
            Err(Error::Os { call, source }) => {
                assert_eq!(
                    (call, source.raw_os_error()),
                    ("posix_spawn", Some(libc::ENOENT))
                );
            }
            other => panic!("{other:?}"),
        }
    }
}
