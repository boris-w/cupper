//! Who is at the other end of a Unix socket (design 5.8.1, ADR-05).
//!
//! The kernel answers for the process that did the latest I/O on the other
//! end, as that process is now, not for the one that connected (SP-4b G2–G4).
//! A process that forks shares the connection with its child, and one that
//! execs talks on as a different program. So a verifier takes the token once,
//! verifies it, keeps it, and compares the token after every read: any
//! difference means another process or another program did I/O since.
//!
//! Any failure to identify or verify a peer is a [`Refusal`]; its kind is for
//! the audit log and the UI's message, never a reason to accept.
//!
//! A peer can also pass fds with its data, and a plain read installs them in
//! the reader's fd table instead of dropping them; [`recv_closing_fds`]
//! closes them.

use std::fmt;
use std::io;
use std::mem;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};

use libc::c_int;

use crate::proc::unique_ids;

/// A process's `audit_token_t`: its pid and ids, and a pid version that
/// changes on every fork and exec, so a token names one program run by one
/// process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AuditToken(pub(crate) [u32; 8]);

impl AuditToken {
    pub fn pid(&self) -> i32 {
        self.0[5] as i32
    }

    pub fn euid(&self) -> u32 {
        self.0[1]
    }

    /// Changes on every fork and exec.
    pub fn pid_version(&self) -> u32 {
        self.0[7]
    }

    /// The unique id of the process the token names (`dev_key` is only
    /// accepted from the host's launcher, design 5.8.1). Refused when that
    /// process has exited or exec'd since the token was taken.
    pub fn unique_id(&self) -> Result<u64, Refusal> {
        match unique_ids(self.pid()) {
            Some(ids) if ids.pid_version == self.pid_version() => Ok(ids.unique_id),
            _ => Err(self.vanished("proc_pidinfo(PROC_PIDUNIQIDENTIFIERINFO)", 0)),
        }
    }

    /// Why a call about the process of this token found no such process:
    /// it exited, or it is now another program (exec). A pid that has been
    /// reused by now counts as exec'd; the peer is refused either way.
    pub(crate) fn vanished(&self, call: &'static str, code: i64) -> Refusal {
        // A zombie has no unique ids any more: it counts as exited.
        let kind = match unique_ids(self.pid()) {
            None => RefusalKind::PeerExited,
            Some(ids) if ids.pid_version != self.pid_version() => RefusalKind::PeerExeced,
            // Still the same program, so the call failed for another reason.
            Some(_) => RefusalKind::Other,
        };
        Refusal { kind, call, code }
    }
}

/// Why a peer was not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind}: {call} returned {code}")]
pub struct Refusal {
    pub kind: RefusalKind,
    /// The call that failed, for the audit log.
    pub call: &'static str,
    /// What it returned: an errno, an `OSStatus`, or the code-signing status
    /// that failed the check.
    pub code: i64,
}

/// The kinds of [`Refusal`]. The first four are design 5.8.1's verification
/// errors; the rest mean verification worked and the peer is not what the
/// requirement asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefusalKind {
    /// The peer has exited (`ENOTCONN`, or no such process).
    PeerExited,
    /// The peer has exec'd another program since its token was taken
    /// (`errSecCSNoSuchCode` while the pid lives on with a new pid version).
    PeerExeced,
    /// The peer's code on disk was removed or replaced, most likely by an
    /// upgrade (`ENOENT`, `errSecCSStaticCodeChanged`).
    CodeChanged,
    /// Anything else.
    Other,
    /// The code does not satisfy the requirement, or is not signed.
    Unsatisfied,
    /// The code runs without the hardened runtime.
    NoRuntime,
    /// The code may be debugged (`get-task-allow`).
    GetTaskAllow,
    /// A debugger has attached to the process.
    Debugged,
}

impl fmt::Display for RefusalKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PeerExited => "the peer has exited",
            Self::PeerExeced => "the peer has exec'd another program",
            Self::CodeChanged => "the peer's code on disk has changed",
            Self::Other => "the peer could not be verified",
            Self::Unsatisfied => "the peer does not satisfy the code requirement",
            Self::NoRuntime => "the peer runs without the hardened runtime",
            Self::GetTaskAllow => "the peer may be debugged",
            Self::Debugged => "the peer has been debugged",
        })
    }
}

/// The audit token of the process that did the latest I/O on the other end
/// of `socket`, a connected Unix socket (`LOCAL_PEERTOKEN`).
pub fn peer_token(socket: impl AsFd) -> Result<AuditToken, Refusal> {
    let socket = socket.as_fd();
    let call = "getsockopt(LOCAL_PEERTOKEN)";
    local_option(socket, libc::LOCAL_PEERTOKEN, call, [0u32; 8])
        .map(AuditToken)
        .map_err(|mut refusal| {
            // `EINVAL` when the process of the latest I/O has exited while
            // another one still holds the other end; `LOCAL_PEERPID` still
            // names it then.
            if refusal.code == i64::from(libc::EINVAL)
                && peer_pid(socket).is_ok_and(|pid| unique_ids(pid).is_none())
            {
                refusal.kind = RefusalKind::PeerExited;
            }
            refusal
        })
}

/// The pid of the process that did the latest I/O on the other end of
/// `socket` (`LOCAL_PEERPID`). It is still reported after that process has
/// exited, when the pid may already name another process: take the pid from
/// a verified [`AuditToken`] instead where it matters.
pub fn peer_pid(socket: impl AsFd) -> Result<i32, Refusal> {
    local_option(
        socket.as_fd(),
        libc::LOCAL_PEERPID,
        "getsockopt(LOCAL_PEERPID)",
        0 as c_int,
    )
}

/// The effective user id the peer had when it connected (`getpeereid`).
pub fn peer_uid(socket: impl AsFd) -> Result<u32, Refusal> {
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    // SAFETY: both out pointers are valid; the call only reads the socket.
    let rc = unsafe { libc::getpeereid(socket.as_fd().as_raw_fd(), &mut uid, &mut gid) };
    if rc != 0 {
        return Err(socket_refusal("getpeereid"));
    }
    Ok(uid)
}

/// A `SOL_LOCAL` option of `socket` of exactly the size of `T`.
fn local_option<T: Copy>(
    socket: BorrowedFd<'_>,
    option: c_int,
    call: &'static str,
    mut value: T,
) -> Result<T, Refusal> {
    let size = mem::size_of::<T>();
    let mut len = size as libc::socklen_t;
    // SAFETY: `value` is a valid, writable `T` of `len` bytes; the kernel
    // writes at most `len` bytes and stores the written length back. `T` is
    // only instantiated with integer types, valid for any bytes written.
    let rc = unsafe {
        libc::getsockopt(
            socket.as_raw_fd(),
            libc::SOL_LOCAL,
            option,
            (&raw mut value).cast(),
            &mut len,
        )
    };
    if rc != 0 {
        return Err(socket_refusal(call));
    }
    if len as usize != size {
        return Err(Refusal {
            kind: RefusalKind::Other,
            call,
            code: i64::from(len),
        });
    }
    Ok(value)
}

/// The refusal for a failed socket call, from `errno`; `ENOTCONN` means the
/// other end is closed.
fn socket_refusal(call: &'static str) -> Refusal {
    let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
    let kind = match errno {
        libc::ENOTCONN => RefusalKind::PeerExited,
        _ => RefusalKind::Other,
    };
    Refusal {
        kind,
        call,
        code: i64::from(errno),
    }
}

/// What [`recv_closing_fds`] read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Received {
    /// Bytes read into the buffer; 0 at end of file.
    pub len: usize,
    /// Fds that came with the data and were closed. A peer that sends any is
    /// not speaking our protocol.
    pub closed_fds: usize,
}

/// Room for the largest control data a sender can attach: the kernel takes
/// at most 2 KiB (`MCLBYTES`), 254 fds per message.
const CONTROL_BYTES: usize = 4096;

/// Reads from a Unix socket like `read(2)`, and closes any fds that came with
/// the data (`SCM_RIGHTS`).
///
/// A plain `read` or `recv` does not make the kernel discard passed fds: it
/// installs them in the reader's fd table, without close-on-exec, and says
/// nothing (macOS 26.4, IT-sys-peer-current). So every read of a socket a
/// peer could pass fds on goes through here. The socket may be non-blocking;
/// `WouldBlock` and every other error is returned as is.
pub fn recv_closing_fds(socket: impl AsFd, buf: &mut [u8]) -> io::Result<Received> {
    let mut iov = libc::iovec {
        iov_base: buf.as_mut_ptr().cast(),
        iov_len: buf.len(),
    };
    // u64 words keep the buffer aligned for `cmsghdr`.
    let mut control = [0u64; CONTROL_BYTES / 8];
    // SAFETY: msghdr is integers and pointers; all-zero is a valid value.
    let mut msg: libc::msghdr = unsafe { mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = CONTROL_BYTES as libc::socklen_t;
    // SAFETY: `msg` points to `iov` (describing `buf`) and to `control`, all
    // valid and writable for the lengths given, and outliving the call.
    let len = unsafe { libc::recvmsg(socket.as_fd().as_raw_fd(), &mut msg, 0) };
    let Ok(len) = usize::try_from(len) else {
        return Err(io::Error::last_os_error());
    };
    let mut closed_fds = 0;
    // SAFETY: the kernel filled `msg`; CMSG_FIRSTHDR and CMSG_NXTHDR return
    // null or a header inside `control`, and an SCM_RIGHTS header's data is
    // `(cmsg_len - CMSG_LEN(0)) / 4` ints, fds the kernel just installed for
    // us and that nothing else refers to.
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let data = libc::CMSG_DATA(cmsg).cast::<c_int>();
                let bytes = (*cmsg).cmsg_len.saturating_sub(libc::CMSG_LEN(0));
                for i in 0..bytes as usize / mem::size_of::<c_int>() {
                    libc::close(data.add(i).read_unaligned());
                    closed_fds += 1;
                }
            }
            cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
        }
    }
    if msg.msg_flags & libc::MSG_CTRUNC != 0 {
        // Fds that did not fit would stay installed, unseen.
        return Err(io::Error::other("control data truncated"));
    }
    Ok(Received { len, closed_fds })
}

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Read, Write};
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::net::UnixStream;
    use std::process::{Command, Stdio};

    use super::*;
    use crate::current_uid;
    use crate::testutil::{
        Forked, Owned, StdChild, alone, cat, expect_byte, expect_line, fd_lock, open_fds,
        sh_writing_to,
    };

    #[test]
    fn it_sys_peer_current_token_names_the_latest_io_process() {
        let _fds = fd_lock();
        let me = std::process::id() as i32;
        let (server, client) = UnixStream::pair().unwrap();
        assert_eq!(peer_uid(&server).unwrap(), current_uid());

        (&client).write_all(b"a").unwrap();
        expect_byte(&server, b'a');
        let mine = peer_token(&server).unwrap();
        assert_eq!((mine.pid(), peer_pid(&server).unwrap()), (me, me));
        assert_eq!(mine.euid(), current_uid());
        assert_eq!(mine.pid_version(), unique_ids(me).unwrap().pid_version);

        // Another process writes on the same end, then waits on its stdin.
        let mut writer = sh_writing_to("echo b; read x || true", &client);
        let writer_pid = writer.0.id() as i32;
        let mut lines = BufReader::new(&server);
        expect_line(&mut lines, "b");
        let theirs = peer_token(&server).unwrap();
        assert_eq!(
            (theirs.pid(), peer_pid(&server).unwrap()),
            (writer_pid, writer_pid)
        );
        assert_eq!(
            theirs.pid_version(),
            unique_ids(writer_pid).unwrap().pid_version
        );
        assert_ne!(theirs, mine);

        // As soon as we do I/O again, the token is ours again.
        (&client).write_all(b"c").unwrap();
        expect_byte(lines.get_ref(), b'c');
        assert_eq!(peer_token(&server).unwrap(), mine);
        drop(writer.0.stdin.take());
        assert!(writer.0.wait().unwrap().success());

        // Reading is I/O too: another sh reads a line from the same end.
        let reader_cmd = Command::new("/bin/sh")
            .args(["-c", "read x; echo \"$x\"; read y || true"])
            .stdin(OwnedFd::from(client.try_clone().unwrap()))
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = StdChild(reader_cmd);
        (&server).write_all(b"d\n").unwrap();
        expect_line(&mut BufReader::new(reader.0.stdout.take().unwrap()), "d");
        assert_eq!(peer_token(&server).unwrap().pid(), reader.0.id() as i32);
        drop(lines);
        drop((server, client));
        assert!(reader.0.wait().unwrap().success());
    }

    #[test]
    fn it_sys_peer_current_exec_changes_the_token() {
        let _fds = fd_lock();
        let (server, client) = UnixStream::pair().unwrap();
        let mut child = sh_writing_to(
            "echo before; read go; exec /bin/sh -c 'echo after; read go || true'",
            &client,
        );
        drop(client);
        let pid = child.0.id() as i32;
        let mut go = child.0.stdin.take().unwrap();
        let mut lines = BufReader::new(&server);
        expect_line(&mut lines, "before");
        let pinned = peer_token(&server).unwrap();
        assert_eq!(pinned.pid(), pid);
        let unique_id = pinned.unique_id().unwrap();

        go.write_all(b"go\n").unwrap();
        expect_line(&mut lines, "after");
        let now = peer_token(&server).unwrap();
        assert_eq!(now.pid(), pid);
        assert_ne!(now.pid_version(), pinned.pid_version());
        assert_ne!(now, pinned);
        assert_eq!(now.unique_id().unwrap(), unique_id);
        let refusal = pinned.unique_id().unwrap_err();
        assert_eq!(refusal.kind, RefusalKind::PeerExeced, "{refusal}");

        drop(go);
        assert!(child.0.wait().unwrap().success());
    }

    #[test]
    fn it_sys_peer_current_forked_child_sharing_the_fd_changes_the_token() {
        let _fds = fd_lock();
        let (server, client) = UnixStream::pair().unwrap();
        let (hold_read, hold_write) = io::pipe().unwrap();
        (&client).write_all(b"p").unwrap();
        expect_byte(&server, b'p');
        let pinned = peer_token(&server).unwrap();
        assert_eq!(pinned.pid(), std::process::id() as i32);

        let (socket, hold_read_fd, hold_write_fd) = (
            client.as_raw_fd(),
            hold_read.as_raw_fd(),
            hold_write.as_raw_fd(),
        );
        // SAFETY: the child of this multi-threaded process only makes
        // async-signal-safe calls (close, write, read, _exit) on fds it
        // inherited, with buffers on its own stack.
        let pid = unsafe { libc::fork() };
        if pid == 0 {
            // SAFETY: as above.
            unsafe {
                libc::close(hold_write_fd);
                libc::write(socket, b"c".as_ptr().cast(), 1);
                let mut byte = 0u8;
                libc::read(hold_read_fd, (&raw mut byte).cast(), 1);
                libc::_exit(0);
            }
        }
        assert!(pid > 0, "fork: {}", io::Error::last_os_error());
        let mut child = Forked(pid);

        expect_byte(&server, b'c');
        let now = peer_token(&server).unwrap();
        assert_eq!(now.pid(), pid);
        assert_ne!(now, pinned);

        // The token only tells who did the latest I/O: once the parent talks
        // again, it is the parent's again. The check must follow every read.
        (&client).write_all(b"p").unwrap();
        expect_byte(&server, b'p');
        assert_eq!(peer_token(&server).unwrap(), pinned);

        drop(hold_write);
        assert_eq!(child.wait(), 0);
    }

    /// Sends `byte` with `fds` attached as one `SCM_RIGHTS` message.
    fn send_fds(socket: &UnixStream, fds: &[BorrowedFd<'_>], byte: u8) -> io::Result<()> {
        let mut data = [byte];
        let mut iov = libc::iovec {
            iov_base: data.as_mut_ptr().cast(),
            iov_len: 1,
        };
        let bytes = mem::size_of_val(fds) as u32;
        // SAFETY: CMSG_SPACE only computes a size.
        let space = unsafe { libc::CMSG_SPACE(bytes) };
        // u64 words keep the buffer aligned for `cmsghdr`.
        let mut control = vec![0u64; (space as usize).div_ceil(8)];
        // SAFETY: msghdr is integers and pointers; all-zero is a valid value.
        let mut msg: libc::msghdr = unsafe { mem::zeroed() };
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = space;
        // SAFETY: `control` has room for one cmsghdr with `bytes` of data, so
        // the header and the `bytes` after CMSG_DATA lie inside it.
        unsafe {
            let cmsg = libc::CMSG_FIRSTHDR(&msg);
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(bytes);
            let data = libc::CMSG_DATA(cmsg).cast::<c_int>();
            for (i, fd) in fds.iter().enumerate() {
                data.add(i).write_unaligned(fd.as_raw_fd());
            }
        }
        // SAFETY: `msg` and everything it points to outlive the call.
        let sent = unsafe { libc::sendmsg(socket.as_raw_fd(), &msg, 0) };
        if sent == 1 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Whether every write end of the pipe of `reader` is closed: a
    /// non-blocking read then reports end of file instead of `EAGAIN`.
    fn writers_gone(reader: &io::PipeReader) -> bool {
        let mut byte = [0u8];
        match (&*reader).read(&mut byte) {
            Ok(0) => true,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => false,
            other => panic!("{other:?}"),
        }
    }

    fn nonblocking_pipe() -> (io::PipeReader, io::PipeWriter) {
        let (reader, writer) = io::pipe().unwrap();
        // SAFETY: fcntl on an fd we own, with plain integer arguments.
        let rc = unsafe { libc::fcntl(reader.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) };
        assert_eq!(rc, 0);
        (reader, writer)
    }

    /// `(st_dev, st_ino)` of the open file `fd`.
    fn file_id(fd: BorrowedFd<'_>) -> Option<(i32, u64)> {
        // SAFETY: stat is plain data; all-zero is a valid value.
        let mut st: libc::stat = unsafe { mem::zeroed() };
        // SAFETY: a valid out pointer; fstat only reads the fd.
        let rc = unsafe { libc::fstat(fd.as_raw_fd(), &mut st) };
        (rc == 0).then_some((st.st_dev, st.st_ino))
    }

    /// The fds of this process that refer to the open file `id`. Only the
    /// copies the kernel installed unannounced do, so they are ours to close.
    fn installed_copies(id: (i32, u64)) -> Vec<OwnedFd> {
        open_fds(std::process::id() as i32)
            .into_iter()
            .filter(|&fd| {
                // SAFETY: only borrowed for the fstat; an fd that another
                // thread closes meanwhile just fails it.
                file_id(unsafe { BorrowedFd::borrow_raw(fd) }) == Some(id)
            })
            // SAFETY: the kernel installed these copies for us and nothing
            // else in the process refers to them.
            .map(|fd| unsafe { OwnedFd::from_raw_fd(fd) })
            .collect()
    }

    fn read_with_std(mut socket: &UnixStream) -> u8 {
        let mut byte = [0u8];
        socket.read_exact(&mut byte).unwrap();
        byte[0]
    }

    fn read_with_libc(socket: &UnixStream) -> u8 {
        let mut byte = 0u8;
        // SAFETY: a one-byte buffer for a one-byte read on an fd we own.
        let n = unsafe { libc::read(socket.as_raw_fd(), (&raw mut byte).cast(), 1) };
        assert_eq!(n, 1, "read: {}", io::Error::last_os_error());
        byte
    }

    #[test]
    fn it_sys_peer_current_plain_reads_install_passed_fds() {
        // Whether the pipe's write end is open anywhere is only knowable while
        // no other test starts a process: each spawn briefly holds a copy.
        if !alone("peer::tests::it_sys_peer_current_plain_reads_install_passed_fds") {
            return;
        }
        // A pipe's write end is passed along with one byte and closed here.
        // The kernel does not drop the passed copy when the reader asks for no
        // control data: std's read (recv(2), what tokio ends up calling) and
        // read(2) both install it in the reader's fd table, unannounced and
        // without close-on-exec. Hence `recv_closing_fds`.
        let readers: [fn(&UnixStream) -> u8; 2] = [read_with_std, read_with_libc];
        for read in readers {
            let (server, client) = UnixStream::pair().unwrap();
            let (pipe, writer) = nonblocking_pipe();
            let id = file_id(writer.as_fd()).unwrap();
            send_fds(&client, &[writer.as_fd()], b'x').unwrap();
            drop(writer);
            assert_eq!(read(&server), b'x');
            assert!(!writers_gone(&pipe), "the passed copy is still open");
            let installed = installed_copies(id);
            assert_eq!(installed.len(), 1, "the passed copy is in our fd table");
            // SAFETY: fcntl F_GETFD on an fd we own.
            let flags = unsafe { libc::fcntl(installed[0].as_raw_fd(), libc::F_GETFD) };
            assert_eq!(flags & libc::FD_CLOEXEC, 0);
            drop(installed);
            assert!(writers_gone(&pipe));
        }
    }

    #[test]
    fn it_sys_peer_current_recv_closes_passed_fds() {
        // See `it_sys_peer_current_plain_reads_install_passed_fds`.
        if !alone("peer::tests::it_sys_peer_current_recv_closes_passed_fds") {
            return;
        }
        let (server, client) = UnixStream::pair().unwrap();
        let (pipe, writer) = nonblocking_pipe();
        // One fd, then the most one message can carry: one more is refused,
        // so the control buffer of `recv_closing_fds` is never too small.
        send_fds(&client, &[writer.as_fd()], b'a').unwrap();
        send_fds(&client, &[writer.as_fd(); 254], b'b').unwrap();
        let refused = send_fds(&client, &[writer.as_fd(); 255], b'-').unwrap_err();
        assert_eq!(refused.raw_os_error(), Some(libc::EINVAL));
        drop(writer);
        (&client).write_all(b"plain").unwrap();

        let mut data = Vec::new();
        let mut closed = 0;
        let mut buf = [0u8; 16];
        while data.len() < 7 {
            let got = recv_closing_fds(&server, &mut buf).unwrap();
            assert!(got.len > 0);
            data.extend_from_slice(&buf[..got.len]);
            closed += got.closed_fds;
        }
        assert_eq!((data.as_slice(), closed), (&b"abplain"[..], 255));
        assert!(writers_gone(&pipe), "no passed copy is left open");

        server.set_nonblocking(true).unwrap();
        let err = recv_closing_fds(&server, &mut buf).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
        drop(client);
        let eof = recv_closing_fds(&server, &mut buf).unwrap();
        assert_eq!(
            eof,
            Received {
                len: 0,
                closed_fds: 0
            }
        );
    }

    #[test]
    fn it_sys_peer_current_exited_peer_is_refused() {
        let _fds = fd_lock();
        let (server, client) = UnixStream::pair().unwrap();
        let mut child = sh_writing_to("echo x; read y || true", &client);
        let mut lines = BufReader::new(&server);
        expect_line(&mut lines, "x");
        let token = peer_token(&server).unwrap();
        assert_eq!(token.pid(), child.0.id() as i32);
        drop(child.0.stdin.take());
        assert!(child.0.wait().unwrap().success());

        // We still hold the other end, but the process of its latest I/O is
        // gone.
        let refusal = peer_token(&server).unwrap_err();
        assert_eq!(refusal.kind, RefusalKind::PeerExited, "{refusal}");
        let refusal = token.unique_id().unwrap_err();
        assert_eq!(refusal.kind, RefusalKind::PeerExited, "{refusal}");

        // Nobody holds the other end any more.
        drop(client);
        let refusal = peer_token(&server).unwrap_err();
        assert_eq!(
            (refusal.kind, refusal.code),
            (RefusalKind::PeerExited, i64::from(libc::ENOTCONN)),
            "{refusal}"
        );
        assert_eq!(peer_pid(&server).unwrap_err().kind, RefusalKind::PeerExited);
    }

    #[test]
    fn unit_sys_uniqueid_peer_token_names_the_launcher() {
        let _fds = fd_lock();
        // The "host" records its launcher, this process, which connects.
        let host = Owned::spawn(cat().new_session());
        let launcher = unique_ids(host.pid()).unwrap().parent_unique_id;
        let (server, client) = UnixStream::pair().unwrap();
        (&client).write_all(b"k").unwrap();
        expect_byte(&server, b'k');
        assert_eq!(peer_token(&server).unwrap().unique_id().unwrap(), launcher);

        // Another process of the same user and session is not the launcher.
        let mut other = sh_writing_to("echo k; read x || true", &client);
        let mut lines = BufReader::new(&server);
        expect_line(&mut lines, "k");
        assert_ne!(peer_token(&server).unwrap().unique_id().unwrap(), launcher);
        drop(other.0.stdin.take());
        assert!(other.0.wait().unwrap().success());
    }
}
