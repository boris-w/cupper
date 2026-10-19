//! Helpers for the tests of this crate. Tests start only harmless programs
//! (`cat`, `sleep`, `sh -c`, `env`, `codesign` on copies of the test binary
//! in a temporary directory) and leave no process or file behind.

use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::mem;
use std::ops::{Deref, DerefMut};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::{Child, Command, ProcessGroup, Signal, Stdio, spawn};

/// Serialises the tests that leave fds without close-on-exec in this process
/// (passed fds) with the tests whose children inherit such fds (`std::process`,
/// `fork`, a spawn without `CLOEXEC_DEFAULT`), so that neither sees the
/// other's fds. Children started with [`spawn`] inherit nothing and need no
/// lock.
pub fn fd_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A child started with `std::process`, for tests that hand it a socket as
/// stdio; killed and waited for when the test ends. Take [`fd_lock`] first.
pub struct StdChild(pub std::process::Child);

impl Drop for StdChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A child made with `fork`; killed and waited for when the test ends, unless
/// [`Forked::wait`] already did.
pub struct Forked(pub i32);

impl Forked {
    /// Waits for the child; its exit status.
    pub fn wait(&mut self) -> i32 {
        let mut status = 0;
        // SAFETY: a valid out pointer; the pid is our unreaped child.
        let rc = unsafe { libc::waitpid(self.0, &mut status, 0) };
        assert_eq!(rc, self.0, "waitpid: {}", std::io::Error::last_os_error());
        self.0 = 0;
        status
    }
}

impl Drop for Forked {
    fn drop(&mut self) {
        if self.0 > 0 {
            let mut status = 0;
            // SAFETY: plain integers and a valid out pointer; the pid is our
            // unreaped child.
            unsafe {
                libc::kill(self.0, libc::SIGKILL);
                libc::waitpid(self.0, &mut status, 0);
            }
        }
    }
}

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

/// A child that is killed, with every group of its process tree, and waited
/// for when the test ends, even when it fails.
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
        if matches!(self.0.try_wait(), Ok(None)) {
            for group in self.0.group().with_descendants() {
                let _ = group.signal(Signal::Kill);
            }
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

/// Process groups that are killed when the test ends, even when it fails.
pub struct Groups(pub Vec<ProcessGroup>);

impl Drop for Groups {
    fn drop(&mut self) {
        for group in &self.0 {
            let _ = group.signal(Signal::Kill);
        }
    }
}

/// Polls `done` until it holds or `limit` passes; whether it held.
#[expect(
    clippy::disallowed_methods,
    reason = "tests poll real processes; cupper-sys has no Clock"
)]
pub fn eventually(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A new directory under the user's temporary directory, removed with its
/// content when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "cupper-sys-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Reads one line and checks it, without its line break.
pub fn expect_line(lines: &mut impl BufRead, want: &str) {
    let mut line = String::new();
    lines.read_line(&mut line).unwrap();
    assert_eq!(line.trim_end(), want);
}

/// Reads one byte from `socket` and checks it.
pub fn expect_byte(mut socket: &UnixStream, want: u8) {
    let mut byte = [0u8];
    socket.read_exact(&mut byte).unwrap();
    assert_eq!(byte[0], want);
}

/// `sh -c script` with `stdout` as its stdout and a pipe as its stdin. Take
/// [`fd_lock`] first.
pub fn sh_writing_to(script: &str, stdout: &UnixStream) -> StdChild {
    let stdout = OwnedFd::from(stdout.try_clone().unwrap());
    StdChild(
        std::process::Command::new("/bin/sh")
            .args(["-c", script])
            .stdin(std::process::Stdio::piped())
            .stdout(stdout)
            .spawn()
            .unwrap(),
    )
}

/// How [`signed_probe`] signs its copy, ad hoc: no certificate or keychain
/// is involved.
#[derive(Clone, Copy)]
pub struct Signing {
    pub identifier: &'static str,
    /// The hardened runtime (`--options runtime`).
    pub runtime: bool,
    /// The `com.apple.security.get-task-allow` entitlement.
    pub get_task_allow: bool,
}

/// The identifier of the probes the requirements of the tests ask for.
pub const PROBE_ID: &str = "dev.cupper.test.peer";

/// Signed the way a release build is checked for: hardened runtime, no
/// `get-task-allow`.
pub const HARDENED: Signing = Signing {
    identifier: PROBE_ID,
    runtime: true,
    get_task_allow: false,
};

const GET_TASK_ALLOW: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>com.apple.security.get-task-allow</key><true/></dict></plist>
"#;

/// A copy of this test binary at `dir/name`, signed as `signing`. Started
/// with [`start_probe`], it is a peer with a signature of our choosing. Take
/// [`fd_lock`] first.
pub fn signed_probe(dir: &Path, name: &str, signing: Signing) -> PathBuf {
    let path = dir.join(name);
    std::fs::copy(std::env::current_exe().unwrap(), &path).unwrap();
    let mut codesign = std::process::Command::new("/usr/bin/codesign");
    codesign.args(["--force", "--sign", "-", "--identifier", signing.identifier]);
    if signing.runtime {
        codesign.args(["--options", "runtime"]);
    }
    if signing.get_task_allow {
        let entitlements = dir.join(format!("{name}.entitlements"));
        std::fs::write(&entitlements, GET_TASK_ALLOW).unwrap();
        codesign.arg("--entitlements").arg(entitlements);
    }
    let out = codesign.arg(&path).output().unwrap();
    assert!(
        out.status.success(),
        "codesign: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    path
}

/// The cdhash `codesign` reports for `path`, as an independent reading.
pub fn codesign_cdhash(path: &Path) -> String {
    let out = std::process::Command::new("/usr/bin/codesign")
        .args(["--display", "--verbose=3"])
        .arg(path)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    text.lines()
        .find_map(|line| line.strip_prefix("CDHash="))
        .unwrap_or_else(|| panic!("no CDHash in {text}"))
        .to_owned()
}

const PROBE_ENV: &str = "SYS_TEST_PEER_PROBE";

/// Starts `probe` (see [`signed_probe`]) with `socket` as its stdin and
/// returns once it has echoed a byte: it is running and did the latest I/O
/// on `socket`. Take [`fd_lock`] first.
pub fn start_probe(probe: &Path, socket: UnixStream, ours: &UnixStream) -> StdChild {
    let child = std::process::Command::new(probe)
        .args([
            "testutil::signed_peer_probe",
            "--exact",
            "--ignored",
            "--test-threads=1",
        ])
        .env(PROBE_ENV, "1")
        .stdin(OwnedFd::from(socket))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let child = StdChild(child);
    (&*ours).write_all(b"?").unwrap();
    expect_byte(ours, b'?');
    child
}

/// The probe itself: echoes what arrives on the socket at fd 0 until end of
/// file. Only does so when started by [`start_probe`].
#[test]
#[ignore = "the signed peer that the IT-sys-peer-current tests start"]
fn signed_peer_probe() {
    if std::env::var_os(PROBE_ENV).is_none() {
        return;
    }
    // SAFETY: fd 0 is the socket `start_probe` passed as stdin, and nothing
    // else in this process uses it.
    let mut socket = UnixStream::from(unsafe { OwnedFd::from_raw_fd(0) });
    let mut buf = [0u8; 64];
    loop {
        let n = socket.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        socket.write_all(&buf[..n]).unwrap();
    }
}
