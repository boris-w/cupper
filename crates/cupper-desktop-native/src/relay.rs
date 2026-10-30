//! The connection to the host as the main process gets it (design 5.7.2,
//! 5.8.1, 5.8.5).
//!
//! Node's `net.Socket` reads with plain `read(2)`, which installs fds the
//! peer passes (T021). So the host's socket never reaches JavaScript: the
//! main process gets one end of a socket pair to wrap in `net.Socket`, and two
//! threads here relay between the other end and the host. Every read of the
//! host goes through `recv_closing_fds` and is followed by a comparison of the
//! host's token with the pinned one; its data reaches the main process only
//! after both. Any refusal ends the connection both ways, and the main
//! process reads end of file and asks for the [`CloseReason`].

use std::io::{self, Write};
use std::net::Shutdown;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex, PoisonError};

use cupper_sys::{AuditToken, peer_token, recv_closing_fds};

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
    /// T207 probe, reverted after the runs on the runner: when `read_host`
    /// left its loop, and when its `close` returned.
    #[cfg(test)]
    probe: Mutex<[Option<std::time::Instant>; 2]>,
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
            #[cfg(test)]
            probe: Mutex::new([None; 2]),
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
            let got = match recv_closing_fds(&self.host, &mut buf) {
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
        #[cfg(test)]
        self.mark(0);
        self.close(reason);
        #[cfg(test)]
        self.mark(1);
    }

    /// T207 probe.
    #[cfg(test)]
    #[expect(clippy::disallowed_methods, reason = "T207 probe")]
    fn mark(&self, at: usize) {
        self.probe.lock().unwrap_or_else(PoisonError::into_inner)[at] =
            Some(std::time::Instant::now());
    }

    fn write_host(&self) {
        let mut buf = vec![0; CHUNK];
        let reason = loop {
            // Our own end: only the main process writes to it.
            match recv_closing_fds(&self.ours, &mut buf) {
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

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::os::fd::AsFd;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use super::*;

    /// A relay to `host`, the other end of a pair: the "host" is this
    /// process, which also pins its token.
    fn started() -> (Arc<Relay>, UnixStream, UnixStream) {
        let (client, host) = UnixStream::pair().unwrap();
        let pinned = peer_token(&client).unwrap();
        let (relay, theirs) = Relay::start(client, pinned).unwrap();
        let theirs = UnixStream::from(theirs);
        // A relay that missed a refusal would leave a read waiting forever.
        for end in [&host, &theirs] {
            end.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        }
        (relay, host, theirs)
    }

    /// Everything the main process reads until end of file.
    fn drain(mut theirs: &UnixStream) -> Vec<u8> {
        let mut data = Vec::new();
        theirs.read_to_end(&mut data).unwrap();
        data
    }

    /// T207 probe, reverted after the runs on the runner: it passes and fails
    /// as on main (no end of file within the 10 s read timeout fails), but
    /// waits on to see when the end of file comes, and appends the timings
    /// to `test-repeat/t207-probe.txt` when test-repeat's directory is there.
    #[test]
    #[expect(clippy::disallowed_methods, reason = "T207 probe")]
    fn unit_native_relay_forwards_both_ways_until_the_host_closes() {
        use std::time::{Instant, SystemTime, UNIX_EPOCH};
        let wall = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let began = Instant::now();
        let watchdog = Watchdog::start();
        let (relay, mut host, mut theirs) = started();
        let ready = Instant::now();
        host.write_all(b"hello").unwrap();
        let mut got = [0; 5];
        theirs.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"hello");
        theirs.write_all(b"initialize").unwrap();
        let mut got = [0; 10];
        host.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"initialize");
        assert_eq!(relay.reason(), None);

        let dropping = Instant::now();
        drop(host);
        let mut data = Vec::new();
        let first = (&theirs).read_to_end(&mut data);
        let drained = Instant::now();
        // On a timeout, wait on to see when the end of file comes, if at all.
        let late = first.as_ref().err().map(|_| {
            theirs
                .set_read_timeout(Some(Duration::from_secs(120)))
                .unwrap();
            let again = (&theirs).read_to_end(&mut data);
            (again.map_err(|err| err.kind()), Instant::now())
        });
        let gaps = watchdog.stop();
        let [broke, closed] = *relay.probe.lock().unwrap();

        let ms = |d: Duration| format!("{:.3}", d.as_secs_f64() * 1e3);
        let after = |at: Option<Instant>| {
            at.map_or("-".to_owned(), |at| {
                ms(at.saturating_duration_since(dropping))
            })
        };
        let me = std::process::id() as i32;
        let since_exec = cupper_sys::process_info(me).map_or("-".to_owned(), |info| {
            let start = Duration::from_micros(info.start.as_micros());
            ms(wall.saturating_sub(start))
        });
        // The watchdog's gaps over 20 ms: the longest, and those that overlap
        // the wait for the end of file.
        let longest = gaps.iter().max_by_key(|(_, gap)| *gap);
        let during: Duration = gaps
            .iter()
            .filter(|(end, gap)| *end > dropping && *end - *gap < drained)
            .map(|(_, gap)| *gap)
            .sum();
        let line = format!(
            "pid={me} since_exec_ms={since_exec} setup_ms={} exchange_ms={} broke_ms={} \
             closed_ms={} drained_ms={} first={} late={} gaps_over_20ms={} longest_gap_ms={} \
             longest_gap_end_ms={} gaps_during_drain_ms={} load={}\n",
            ms(ready - began),
            ms(dropping - ready),
            after(broke),
            after(closed),
            ms(drained - dropping),
            first
                .as_ref()
                .map_or_else(|err| format!("{:?}", err.kind()), |n| format!("ok:{n}")),
            late.as_ref().map_or("-".to_owned(), |(again, at)| format!(
                "{}@{}",
                again
                    .as_ref()
                    .map_or_else(|kind| format!("{kind:?}"), |n| format!("ok:{n}")),
                ms(at.saturating_duration_since(dropping))
            )),
            gaps.len(),
            longest.map_or("-".to_owned(), |(_, gap)| ms(*gap)),
            longest.map_or("-".to_owned(), |(end, _)| {
                if *end >= dropping {
                    ms(*end - dropping)
                } else {
                    format!("-{}", ms(dropping - *end))
                }
            }),
            ms(during),
            load_average(),
        );
        eprint!("T207 {line}");
        let out = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-repeat"));
        if out.is_dir() {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(out.join("t207-probe.txt"))
                .unwrap();
            file.write_all(line.as_bytes()).unwrap();
        }

        first.unwrap();
        assert!(data.is_empty());
        assert_eq!(relay.reason(), Some(CloseReason::HostClosed));
    }

    /// T207 probe: a thread that sleeps 1 ms at a time and keeps each gap
    /// over 20 ms between its wakeups, with when it ended.
    struct Watchdog {
        stop: Arc<std::sync::atomic::AtomicBool>,
        thread: std::thread::JoinHandle<Vec<(std::time::Instant, Duration)>>,
    }

    impl Watchdog {
        #[expect(clippy::disallowed_methods, reason = "T207 probe")]
        fn start() -> Self {
            use std::sync::atomic::Ordering;
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = Arc::clone(&stop);
            let thread = std::thread::spawn(move || {
                let mut gaps = Vec::new();
                let mut last = std::time::Instant::now();
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(1));
                    let now = std::time::Instant::now();
                    if now - last > Duration::from_millis(20) {
                        gaps.push((now, now - last));
                    }
                    last = now;
                }
                gaps
            });
            Self { stop, thread }
        }

        fn stop(self) -> Vec<(std::time::Instant, Duration)> {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            self.thread.join().unwrap()
        }
    }

    /// The 1-minute load average (T207 probe).
    fn load_average() -> String {
        let out = Command::new("/usr/sbin/sysctl")
            .args(["-n", "vm.loadavg"])
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&out.stdout).into_owned();
        out.split_whitespace().nth(1).unwrap_or("?").to_owned()
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

/// T207 probe, reverted after the runs on the runner: the relay's shape with
/// variants of how our end is ended and how the main process waits, each
/// tried many times. A miss: the wait for end of file timed out although the
/// relay had ended our end, and end of file was there when read again. Runs
/// only when test-repeat's directory is there, and appends one line per
/// variant to `test-repeat/t207-variants.txt`.
#[cfg(test)]
#[expect(unsafe_code, reason = "T207 probe: poll and kqueue")]
mod t207_probe {
    use std::io::{ErrorKind, Read, Write};
    use std::net::Shutdown;
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    /// Reads of the mini relay that gave up after 2 s.
    static RELAY_STUCK: AtomicUsize = AtomicUsize::new(0);

    #[derive(Clone, Copy, Debug)]
    enum Closer {
        /// As `Relay::close`: both sockets, both ways, from each thread that
        /// ends.
        Relay,
        /// Only the host thread ends our end, both ways; the other thread
        /// just stops.
        BothOnce,
        /// The host thread shuts our end for writing only; the other thread
        /// stops when the main process closes its end.
        WriteOnly,
        /// The host thread closes our end's fd (it is the only owner; no
        /// other thread).
        CloseFd,
        /// Both ways, then the fd is closed (no other thread).
        BothThenClose,
    }

    #[derive(Clone, Copy, Debug)]
    enum Wait {
        /// A blocking read with `SO_RCVTIMEO`, as the test.
        Read,
        /// `poll(POLLIN)`, then a read.
        Poll,
        /// A level-triggered kqueue `EVFILT_READ` added before the host
        /// closes, as libuv adds it, then a read.
        Kqueue,
    }

    const TIMEOUT: Duration = Duration::from_millis(100);

    /// The test's steps on the real [`super::Relay`], with a blocking read.
    fn real_relay() -> usize {
        let (client, mut host) = UnixStream::pair().unwrap();
        let pinned = cupper_sys::peer_token(&client).unwrap();
        let (relay, theirs) = super::Relay::start(client, pinned).unwrap();
        let mut theirs = UnixStream::from(theirs);
        theirs
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        host.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        host.write_all(b"hello").unwrap();
        let mut got = [0; 5];
        theirs.read_exact(&mut got).unwrap();
        theirs.write_all(b"initialize").unwrap();
        let mut got = [0; 10];
        host.read_exact(&mut got).unwrap();
        theirs.set_read_timeout(Some(TIMEOUT)).unwrap();
        drop(host);
        let mut buf = [0; 64];
        let outcome = match theirs.read(&mut buf) {
            Ok(0) => 0,
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                theirs.set_nonblocking(true).unwrap();
                match theirs.read(&mut buf) {
                    Ok(0) => 1,
                    _ => 2,
                }
            }
            other => panic!("read: {other:?}"),
        };
        assert!(relay.reason().is_some());
        outcome
    }

    /// One try: 0 woke with end of file, 1 missed it (there on reading
    /// again), 2 no end of file even then.
    fn trial(closer: Closer, wait: Wait, exchange: bool) -> usize {
        let (client, mut host) = UnixStream::pair().unwrap();
        let (ours, mut theirs) = UnixStream::pair().unwrap();
        // The mini relay's own reads can miss an end of file the same way:
        // they give up after 2 s, counted, instead of hanging the probe.
        for end in [&client, &ours] {
            end.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        }
        let client = Arc::new(client);
        let other = matches!(closer, Closer::Relay | Closer::BothOnce | Closer::WriteOnly);
        let ours = Arc::new(ours);
        let end = move |client: &UnixStream, ours: &UnixStream| {
            let _ = client.shutdown(Shutdown::Both);
            let _ = ours.shutdown(Shutdown::Both);
        };
        let writer = other.then(|| {
            let (client, ours) = (Arc::clone(&client), Arc::clone(&ours));
            std::thread::spawn(move || {
                let mut buf = [0; 256];
                loop {
                    match (&*ours).read(&mut buf) {
                        Err(err) if err.kind() == ErrorKind::WouldBlock => {
                            RELAY_STUCK.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if (&*client).write_all(&buf[..n]).is_err() {
                                break;
                            }
                        }
                    }
                }
                if matches!(closer, Closer::Relay) {
                    end(&client, &ours);
                }
            })
        });
        let reader = {
            let mut ours = Some(ours);
            std::thread::spawn(move || {
                let mut buf = [0; 256];
                loop {
                    match (&*client).read(&mut buf) {
                        Err(err) if err.kind() == ErrorKind::WouldBlock => {
                            RELAY_STUCK.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let ours = ours.as_deref().unwrap();
                            if (&*ours).write_all(&buf[..n]).is_err() {
                                break;
                            }
                        }
                    }
                }
                let ours = ours.take().unwrap();
                match closer {
                    Closer::Relay | Closer::BothOnce => end(&client, &ours),
                    Closer::WriteOnly => {
                        let _ = client.shutdown(Shutdown::Both);
                        let _ = ours.shutdown(Shutdown::Write);
                    }
                    Closer::CloseFd => {
                        let _ = client.shutdown(Shutdown::Both);
                        drop(Arc::into_inner(ours).unwrap());
                    }
                    Closer::BothThenClose => {
                        end(&client, &ours);
                        drop(Arc::into_inner(ours).unwrap());
                    }
                }
            })
        };
        let kq = matches!(wait, Wait::Kqueue).then(|| register(&theirs));
        theirs.set_read_timeout(Some(TIMEOUT)).unwrap();
        host.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        if exchange {
            host.write_all(b"hello").unwrap();
            let mut got = [0; 5];
            theirs.read_exact(&mut got).unwrap();
            if other {
                theirs.write_all(b"initialize").unwrap();
                let mut got = [0; 10];
                host.read_exact(&mut got).unwrap();
            }
        }
        theirs.set_read_timeout(Some(TIMEOUT)).unwrap();
        drop(host);
        let woke = match wait {
            Wait::Read => {
                let mut buf = [0; 64];
                match theirs.read(&mut buf) {
                    Ok(0) => true,
                    Err(err) if err.kind() == ErrorKind::WouldBlock => false,
                    other => panic!("read: {other:?}"),
                }
            }
            Wait::Poll => {
                let mut fd = libc::pollfd {
                    fd: theirs.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: one valid pollfd.
                let rc = unsafe { libc::poll(&mut fd, 1, TIMEOUT.as_millis() as i32) };
                assert!(rc >= 0);
                rc > 0
            }
            Wait::Kqueue => kevent_wait(kq.unwrap()),
        };
        reader.join().unwrap();
        let outcome = if woke {
            0
        } else {
            theirs.set_nonblocking(true).unwrap();
            let mut buf = [0; 64];
            match theirs.read(&mut buf) {
                Ok(0) => 1,
                _ => 2,
            }
        };
        drop(theirs);
        if let Some(writer) = writer {
            writer.join().unwrap();
        }
        if let Some(kq) = kq {
            // SAFETY: our kqueue.
            unsafe { libc::close(kq) };
        }
        outcome
    }

    fn register(socket: &UnixStream) -> i32 {
        // SAFETY: plain calls; `ev` is a valid kevent.
        unsafe {
            let kq = libc::kqueue();
            assert!(kq >= 0);
            let mut ev: libc::kevent = std::mem::zeroed();
            ev.ident = socket.as_raw_fd() as usize;
            ev.filter = libc::EVFILT_READ;
            ev.flags = libc::EV_ADD;
            let rc = libc::kevent(kq, &ev, 1, std::ptr::null_mut(), 0, std::ptr::null());
            assert_eq!(rc, 0);
            kq
        }
    }

    fn kevent_wait(kq: i32) -> bool {
        kevent_wait_for(kq, TIMEOUT)
    }

    fn kevent_wait_for(kq: i32, timeout: Duration) -> bool {
        // SAFETY: plain calls; `ev` and `ts` are valid.
        unsafe {
            let mut ev: libc::kevent = std::mem::zeroed();
            let ts = libc::timespec {
                tv_sec: timeout.as_secs() as _,
                tv_nsec: timeout.subsec_nanos() as _,
            };
            let rc = libc::kevent(kq, std::ptr::null(), 0, &mut ev, 1, &ts);
            assert!(rc >= 0);
            rc > 0
        }
    }

    #[test]
    fn t207_probe_variants() {
        let out = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-repeat"));
        if !out.is_dir() {
            return;
        }
        let variants = [
            (Closer::Relay, Wait::Read, true),
            (Closer::Relay, Wait::Read, false),
            (Closer::Relay, Wait::Poll, true),
            (Closer::Relay, Wait::Kqueue, true),
            (Closer::BothOnce, Wait::Read, true),
            (Closer::WriteOnly, Wait::Read, true),
            (Closer::CloseFd, Wait::Read, true),
            (Closer::BothThenClose, Wait::Read, true),
        ];
        let tries: usize = std::env::var("T207_TRIES").map_or(5000, |n| n.parse().unwrap());
        let mut lines = String::new();
        for (closer, wait, exchange) in variants {
            let mut counts = [0usize; 3];
            for _ in 0..tries {
                counts[trial(closer, wait, exchange)] += 1;
            }
            lines += &format!(
                "pid={} closer={closer:?} wait={wait:?} exchange={exchange} tries={tries} \
                 woke={} missed={} no_eof={} relay_stuck={}\n",
                std::process::id(),
                counts[0],
                counts[1],
                counts[2],
                RELAY_STUCK.swap(0, Ordering::Relaxed)
            );
        }
        let mut counts = [0usize; 3];
        for _ in 0..tries {
            counts[real_relay()] += 1;
        }
        lines += &format!(
            "pid={} closer=RealRelay wait=Read exchange=true tries={tries} woke={} missed={} \
             no_eof={}\n",
            std::process::id(),
            counts[0],
            counts[1],
            counts[2]
        );
        eprint!("{lines}");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(out.join("t207-variants.txt"))
            .unwrap();
        file.write_all(lines.as_bytes()).unwrap();
    }

    #[derive(Clone, Copy, Debug)]
    enum Peer {
        /// The peer closes its fd, as the host does when it exits.
        Close,
        /// The peer's socket is shut both ways, as `Relay::close` does.
        Shutdown,
    }

    #[derive(Clone, Copy, Debug)]
    enum Entry {
        /// `recv_closing_fds` blocking with `SO_RCVTIMEO`, as the relay's
        /// threads read.
        Recv,
        /// `poll(POLLIN)`, then `recv_closing_fds`.
        PollRecv,
        /// A level-triggered kqueue `EVFILT_READ`, then `recv_closing_fds`.
        KqueueRecv,
    }

    /// One race of a reader entering its wait against the peer ending the
    /// connection `delay` after both start: 0 woke with end of file, 1
    /// missed it (there on reading again), 2 no end of file even then.
    #[expect(clippy::disallowed_methods, reason = "T207 probe")]
    fn race(peer: Peer, entry: Entry, delay: Duration) -> usize {
        let (ours, theirs) = UnixStream::pair().unwrap();
        ours.set_read_timeout(Some(TIMEOUT)).unwrap();
        let kq = matches!(entry, Entry::KqueueRecv).then(|| register(&ours));
        let start = Arc::new(Barrier::new(2));
        let reader = {
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                let mut buf = [0; 64];
                start.wait();
                let ready = match entry {
                    Entry::Recv => true,
                    Entry::PollRecv => {
                        let mut fd = libc::pollfd {
                            fd: ours.as_raw_fd(),
                            events: libc::POLLIN,
                            revents: 0,
                        };
                        // SAFETY: one valid pollfd.
                        let rc = unsafe { libc::poll(&mut fd, 1, TIMEOUT.as_millis() as i32) };
                        assert!(rc >= 0);
                        rc > 0
                    }
                    Entry::KqueueRecv => kevent_wait(kq.unwrap()),
                };
                let woke = ready
                    && match cupper_sys::recv_closing_fds(&ours, &mut buf) {
                        Ok(got) => {
                            assert_eq!(got.len, 0);
                            true
                        }
                        Err(err) if err.kind() == ErrorKind::WouldBlock => false,
                        Err(err) => panic!("recv: {err}"),
                    };
                if let Some(kq) = kq {
                    // SAFETY: our kqueue.
                    unsafe { libc::close(kq) };
                }
                if woke {
                    return 0;
                }
                ours.set_nonblocking(true).unwrap();
                match cupper_sys::recv_closing_fds(&ours, &mut buf) {
                    Ok(got) if got.len == 0 => 1,
                    _ => 2,
                }
            })
        };
        start.wait();
        let until = Instant::now() + delay;
        while Instant::now() < until {
            std::hint::spin_loop();
        }
        match peer {
            Peer::Close => drop(theirs),
            Peer::Shutdown => {
                theirs.shutdown(Shutdown::Both).unwrap();
                let outcome = reader.join().unwrap();
                drop(theirs);
                return outcome;
            }
        }
        reader.join().unwrap()
    }

    #[test]
    fn t207_probe_entry_races() {
        let out = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-repeat"));
        if !out.is_dir() {
            return;
        }
        let tries: usize = std::env::var("T207_RACES").map_or(20_000, |n| n.parse().unwrap());
        let mut lines = String::new();
        for peer in [Peer::Close, Peer::Shutdown] {
            for entry in [Entry::Recv, Entry::PollRecv, Entry::KqueueRecv] {
                let mut counts = [0usize; 3];
                // The peer ends the connection 0 to 30 µs after the reader
                // starts to read, in 100 ns steps.
                for i in 0..tries {
                    let delay = Duration::from_nanos((i % 301) as u64 * 100);
                    counts[race(peer, entry, delay)] += 1;
                }
                lines += &format!(
                    "pid={} peer={peer:?} entry={entry:?} tries={tries} woke={} missed={} \
                     no_eof={}\n",
                    std::process::id(),
                    counts[0],
                    counts[1],
                    counts[2]
                );
            }
        }
        eprint!("{lines}");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(out.join("t207-races.txt"))
            .unwrap();
        file.write_all(lines.as_bytes()).unwrap();
    }

    /// How long after its `shutdown(Write)` the host closes the fd in
    /// [`relay_eof`]: long enough to tell the two apart.
    const CLOSE_AFTER: Duration = Duration::from_millis(5);

    /// The real relay, with the host ending a connection as its gateway
    /// does: a last frame, then `shutdown(Write)` (its `OwnedWriteHalf`
    /// dropped) `gap` later, then the fd closed [`CLOSE_AFTER`] after that
    /// (the read half dropped) — or, with `half` false, just the fd closed
    /// `gap` after the frame. The main process waits as libuv does
    /// (kqueue, non-blocking reads). Returns the µs from the host's first
    /// end to end of file at the main process.
    #[expect(clippy::disallowed_methods, reason = "T207 probe")]
    fn relay_eof(half: bool, gap: Duration) -> u128 {
        let (client, host) = UnixStream::pair().unwrap();
        let pinned = cupper_sys::peer_token(&client).unwrap();
        let (_relay, theirs) = super::Relay::start(client, pinned).unwrap();
        let theirs = UnixStream::from(theirs);
        theirs.set_nonblocking(true).unwrap();
        let kq = register(&theirs);
        (&host).write_all(b"x").unwrap();
        let until = Instant::now() + gap;
        while Instant::now() < until {
            std::hint::spin_loop();
        }
        let ended = Instant::now();
        let closer = if half {
            host.shutdown(Shutdown::Write).unwrap();
            Some(std::thread::spawn(move || {
                std::thread::sleep(CLOSE_AFTER);
                drop(host);
            }))
        } else {
            drop(host);
            None
        };
        let mut buf = [0; 64];
        let eof = loop {
            match (&theirs).read(&mut buf) {
                Ok(0) => break Instant::now(),
                Ok(_) => {}
                Err(err) if err.kind() == ErrorKind::WouldBlock => {
                    assert!(
                        kevent_wait_for(kq, Duration::from_secs(10)),
                        "no end of file"
                    );
                }
                Err(err) => panic!("read: {err}"),
            }
        };
        if let Some(closer) = closer {
            closer.join().unwrap();
        }
        // SAFETY: our kqueue.
        unsafe { libc::close(kq) };
        (eof - ended).as_micros()
    }

    #[test]
    fn t207_probe_relay_eof() {
        let out = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-repeat"));
        if !out.is_dir() {
            return;
        }
        let tries: usize = std::env::var("T207_EOFS").map_or(4000, |n| n.parse().unwrap());
        let mut lines = String::new();
        for half in [true, false] {
            let mut took: Vec<u128> = (0..tries)
                // The host ends 0 to 40 µs after its last frame, in 200 ns
                // steps.
                .map(|i| relay_eof(half, Duration::from_nanos((i % 201) as u64 * 200)))
                .collect();
            took.sort_unstable();
            let at = |q: f64| took[((took.len() - 1) as f64 * q) as usize];
            let over = |us: u128| took.iter().filter(|t| **t >= us).count();
            lines += &format!(
                "pid={} half_close_first={half} tries={tries} p50_us={} p90_us={} p99_us={} \
                 p999_us={} max_us={} over_1ms={} over_4ms={} over_10ms={}\n",
                std::process::id(),
                at(0.5),
                at(0.9),
                at(0.99),
                at(0.999),
                took[took.len() - 1],
                over(1_000),
                over(4_000),
                over(10_000)
            );
        }
        eprint!("{lines}");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(out.join("t207-relay-eof.txt"))
            .unwrap();
        file.write_all(lines.as_bytes()).unwrap();
    }
}
