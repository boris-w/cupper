//! Helpers for the tests of this crate. Tests start only harmless programs
//! (`cat`, `sleep`, `sh -c`, `env`) and leave no process behind.

use std::fs::File;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use crate::{Child, Command, Stdio, spawn};

/// `/bin/cat` reading a pipe and writing a pipe: it stays alive until we close
/// its stdin.
pub fn cat() -> Command {
    let mut cmd = Command::new("/bin/cat");
    cmd.stdin(Stdio::Pipe).stdout(Stdio::Pipe);
    cmd
}

/// `sh -c script` with stdout on a pipe.
pub fn sh(script: &str) -> Command {
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", script]).stdout(Stdio::Pipe);
    cmd
}

/// A child that is killed with its process group and waited for when the
/// test ends, even when it fails.
pub struct Owned(pub Child);

impl Owned {
    pub fn spawn(cmd: &Command) -> Self {
        Self(spawn(cmd).expect("spawn"))
    }

    /// Our end of the child's stdin as a file.
    pub fn stdin_file(&mut self) -> File {
        File::from(self.0.stdin.take().expect("stdin is a pipe"))
    }

    /// Our end of the child's stdout as a file.
    pub fn stdout_file(&mut self) -> File {
        File::from(self.0.stdout.take().expect("stdout is a pipe"))
    }
}

impl Deref for Owned {
    type Target = Child;

    fn deref(&self) -> &Child {
        &self.0
    }
}

impl DerefMut for Owned {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // Until it is waited for, the child's pid, and so its group id, cannot
        // be reused, so the signal cannot reach anyone else.
        if matches!(self.0.try_wait(), Ok(None)) {
            // SAFETY: killpg takes plain integers.
            unsafe { libc::killpg(self.0.pid(), libc::SIGKILL) };
            let _ = self.0.wait();
        }
    }
}

/// The open fds of `pid`, a process of ours.
pub fn open_fds(pid: i32) -> Vec<i32> {
    // SAFETY: proc_fdinfo is two integers; all-zero bytes are a valid value.
    let mut fds = vec![unsafe { mem::zeroed::<libc::proc_fdinfo>() }; 1024];
    let size = (fds.len() * mem::size_of::<libc::proc_fdinfo>()) as libc::c_int;
    // SAFETY: the buffer holds `size` bytes and the kernel writes at most that.
    let written =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, fds.as_mut_ptr().cast(), size) };
    assert!(written > 0, "proc_pidinfo(PROC_PIDLISTFDS) of {pid}");
    fds.truncate(written as usize / mem::size_of::<libc::proc_fdinfo>());
    fds.iter().map(|fd| fd.proc_fd).collect()
}

/// An open fd without `FD_CLOEXEC`, as a careless library might leave one.
pub fn inheritable_fd() -> OwnedFd {
    let null = File::open("/dev/null").unwrap();
    // SAFETY: dup takes a plain fd, which `null` keeps open during the call.
    let fd = unsafe { libc::dup(null.as_raw_fd()) };
    assert!(fd > 2, "dup: {}", std::io::Error::last_os_error());
    // SAFETY: `fd` was just returned by dup and has no other owner.
    unsafe { OwnedFd::from_raw_fd(fd) }
}
