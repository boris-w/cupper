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
