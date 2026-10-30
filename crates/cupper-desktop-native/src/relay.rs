//! The connection to the host as the main process gets it (design 5.7.2,
//! 5.8.1, 5.8.5).
//!
//! Node's `net.Socket` reads with plain `read(2)`, which installs fds the
//! peer passes (T021). So the host's socket never reaches JavaScript: the
//! main process gets one end of a socket pair to wrap in `net.Socket`, and two
//! threads here relay between the other end and the host. Every read of the
//! host goes through `recv_closing_fds` and is followed by a comparison of the
//! host's token with the pinned one; its data reaches the main process only
//! after both. Each thread waits for its socket to be readable before it
//! reads (`recv`). Any refusal ends the connection both ways, and the main
//! process reads end of file and asks for the [`CloseReason`].

use std::io::{self, Write};
use std::net::Shutdown;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex, PoisonError};

use cupper_sys::{AuditToken, Received, peer_token, recv_closing_fds, wait_readable};

/// Bytes read at a time, either way.
const CHUNK: usize = 64 * 1024;

/// Why the connection ended; the first reason stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// The host closed the connection, or it failed.
    HostClosed,
    /// The host passed fds with its data (design 5.8.5).
    FdsPassed,
    /// Another process, or another program, did I/O on the host's end since
    /// the token was pinned, or the token could not be read (design 5.8.1).
    TokenChanged,
    /// `host_proof` was missing or wrong (design 5.8.1).
    #[cfg(feature = "dev-auth")]
    HostUnverified,
    /// The main process closed its end, or asked to close.
    Local,
}

impl CloseReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::HostClosed => "host_closed",
            Self::FdsPassed => "fds_passed",
            Self::TokenChanged => "token_changed",
            #[cfg(feature = "dev-auth")]
            Self::HostUnverified => "host_unverified",
            Self::Local => "local",
        }
    }
}

/// A running relay. Writing to a closed socket raises `SIGPIPE`, which Node
/// and Electron ignore; the write then fails with `EPIPE`.
#[derive(Debug)]
pub struct Relay {
    host: UnixStream,
    ours: UnixStream,
    pinned: AuditToken,
    reason: Mutex<Option<CloseReason>>,
}

impl Relay {
    /// Starts relaying between `host`, whose verified token is `pinned`, and
    /// a new socket pair; returns the relay and the pair's other end, for the
    /// main process.
    pub fn start(host: UnixStream, pinned: AuditToken) -> io::Result<(Arc<Self>, OwnedFd)> {
        let (ours, theirs) = UnixStream::pair()?;
        let relay = Arc::new(Self {
            host,
            ours,
            pinned,
            reason: Mutex::new(None),
        });
        let started = relay
            .spawn("cupper-host-read", Self::read_host)
            .and_then(|()| relay.spawn("cupper-host-write", Self::write_host));
        if let Err(err) = started {
            relay.close(CloseReason::Local);
            return Err(err);
        }
        Ok((relay, theirs.into()))
    }

    /// The token pinned on connect.
    pub fn pinned(&self) -> AuditToken {
        self.pinned
    }

    /// Why the connection ended; `None` while it is open.
    pub fn reason(&self) -> Option<CloseReason> {
        *self.reason.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ends the connection both ways, which also ends both threads.
    pub fn close(&self, reason: CloseReason) {
        self.reason
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(reason);
        // Fails only on a socket that is already shut down.
        let _ = self.host.shutdown(Shutdown::Both);
        let _ = self.ours.shutdown(Shutdown::Both);
    }

    fn spawn(self: &Arc<Self>, name: &str, run: fn(&Self)) -> io::Result<()> {
        let this = Arc::clone(self);
        let thread = std::thread::Builder::new().name(name.into());
        thread.spawn(move || run(&this)).map(drop)
    }

    fn read_host(&self) {
        let mut buf = vec![0; CHUNK];
        let reason = loop {
            let got = match recv(&self.host, &mut buf) {
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break CloseReason::HostClosed,
                Ok(got) => got,
            };
            if got.closed_fds > 0 {
                break CloseReason::FdsPassed;
            }
            if got.len == 0 {
                break CloseReason::HostClosed;
            }
            if peer_token(&self.host) != Ok(self.pinned) {
                break CloseReason::TokenChanged;
            }
            if (&self.ours).write_all(&buf[..got.len]).is_err() {
                break CloseReason::Local;
            }
        };
        self.close(reason);
    }

    fn write_host(&self) {
        let mut buf = vec![0; CHUNK];
        let reason = loop {
            // Our own end: only the main process writes to it.
            match recv(&self.ours, &mut buf) {
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break CloseReason::Local,
                Ok(got) if got.len == 0 => break CloseReason::Local,
                Ok(got) => {
                    if (&self.host).write_all(&buf[..got.len]).is_err() {
                        break CloseReason::HostClosed;
                    }
                }
            }
        };
        self.close(reason);
    }
}

/// `recv_closing_fds` once `socket` is readable. A blocking read can sleep
/// through the peer's shutdown, and the other end would then hear of it only
/// when the peer closes its fd (T207, [`wait_readable`]).
fn recv(socket: &UnixStream, buf: &mut [u8]) -> io::Result<Received> {
    wait_readable(socket, None)?;
    recv_closing_fds(socket, buf)
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::os::fd::AsFd;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use super::*;

    /// How long a test waits on the relay: one that missed a refusal would
    /// leave the wait going forever.
    const PATIENCE: Duration = Duration::from_secs(10);

    /// A relay to `host`, the other end of a pair: the "host" is this
    /// process, which also pins its token.
    fn started() -> (Arc<Relay>, UnixStream, UnixStream) {
        let (client, host) = UnixStream::pair().unwrap();
        let pinned = peer_token(&client).unwrap();
        let (relay, theirs) = Relay::start(client, pinned).unwrap();
        let theirs = UnixStream::from(theirs);
        for end in [&host, &theirs] {
            end.set_read_timeout(Some(PATIENCE)).unwrap();
        }
        (relay, host, theirs)
    }

    /// Everything `end` reads until end of file, waiting as the main process
    /// (libuv) and the host (tokio) do: until it is readable, then reading.
    /// A blocking read can sleep through the relay's shutdown (T207).
    fn drain(mut end: &UnixStream) -> Vec<u8> {
        let (mut data, mut buf) = (Vec::new(), [0; 64]);
        loop {
            let readable = cupper_sys::wait_readable(end, Some(PATIENCE)).unwrap();
            assert!(readable, "no end of file within {PATIENCE:?}");
            match end.read(&mut buf).unwrap() {
                0 => return data,
                got => data.extend_from_slice(&buf[..got]),
            }
        }
    }

    #[test]
    fn unit_native_relay_forwards_both_ways_until_the_host_closes() {
        let (relay, mut host, mut theirs) = started();
        host.write_all(b"hello").unwrap();
        let mut got = [0; 5];
        theirs.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"hello");
        theirs.write_all(b"initialize").unwrap();
        let mut got = [0; 10];
        host.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"initialize");
        assert_eq!(relay.reason(), None);

        drop(host);
        assert!(drain(&theirs).is_empty());
        assert_eq!(relay.reason(), Some(CloseReason::HostClosed));
    }

    /// The host ends a connection with `shutdown(Write)` (its gateway's
    /// `OwnedWriteHalf` dropped) and closes the fd later; here not before
    /// the main process has read end of file. A blocking read can sleep
    /// through that shutdown (T207), so this is a matter of tries: with the
    /// relay reading that way, 0.04–0.15% of tries on the runner came
    /// through only at the close (test-repeat runs 41006408074 and
    /// 41006415634), which 20000 tries all but always catch. The shutdown
    /// comes after 0 to 3980 spins, so that some tries meet the relay as it
    /// goes back to its read after the last frame.
    #[test]
    fn unit_native_relay_passes_the_hosts_shutdown_on_at_once() {
        for i in 0..20_000 {
            let (relay, mut host, theirs) = started();
            host.write_all(b"x").unwrap();
            for _ in 0..i % 200 * 20 {
                std::hint::spin_loop();
            }
            host.shutdown(Shutdown::Write).unwrap();
            assert_eq!(drain(&theirs), b"x", "try {i}");
            assert_eq!(relay.reason(), Some(CloseReason::HostClosed));
        }
    }

    #[test]
    fn unit_native_relay_closing_our_end_closes_the_host_connection() {
        let (relay, host, theirs) = started();
        drop(theirs);
        assert!(drain(&host).is_empty(), "the host reads end of file");
        assert_eq!(relay.reason(), Some(CloseReason::Local));

        let (relay, host, theirs) = started();
        relay.close(CloseReason::TokenChanged);
        assert!(drain(&host).is_empty());
        assert!(drain(&theirs).is_empty());
        relay.close(CloseReason::Local);
        assert_eq!(
            relay.reason(),
            Some(CloseReason::TokenChanged),
            "the first stays"
        );
    }

    #[test]
    fn unit_native_relay_disconnects_when_fds_come() {
        let (relay, host, theirs) = started();
        let (pipe, _writer) = io::pipe().unwrap();
        cupper_sys::send_with_fds(&host, b"x", &[pipe.as_fd()]).unwrap();
        assert!(drain(&theirs).is_empty(), "the data is dropped");
        assert_eq!(relay.reason(), Some(CloseReason::FdsPassed));
        assert!(drain(&host).is_empty(), "the host's connection is closed");
    }

    #[test]
    fn unit_native_relay_disconnects_when_the_token_changes() {
        let (relay, host, theirs) = started();
        // Another process writes on the host's end of the connection.
        let mut sh = Command::new("/bin/sh")
            .args(["-c", "printf x"])
            .stdout(OwnedFd::from(host.try_clone().unwrap()))
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        assert!(drain(&theirs).is_empty(), "the data is dropped");
        assert_eq!(relay.reason(), Some(CloseReason::TokenChanged));
        assert!(sh.wait().unwrap().success());
        assert!(drain(&host).is_empty());
    }
}
