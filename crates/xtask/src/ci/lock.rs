//! Machine-wide lock for the jobs that are sensitive to CPU contention: e2e, `ui`
//! and performance runs (plan 1.8). Several agents and the orchestrator run CI on
//! one machine; these jobs queue behind each other instead of skewing timings.
//!
//! The lock is an advisory `flock` on a file in the per-user temporary directory.
//! The kernel drops it when the holder's file descriptor closes, including when
//! the process is killed, so a crashed run never leaves a stale lock behind.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

pub struct MachineLock {
    _file: File,
}

impl MachineLock {
    /// `$TMPDIR/cupper-ci/machine.lock`; on macOS `$TMPDIR` is private to the user.
    pub fn default_path() -> PathBuf {
        std::env::temp_dir().join("cupper-ci").join("machine.lock")
    }

    /// Blocks until this process holds the lock. `on_wait` runs once, before
    /// blocking, if another holder is seen.
    pub fn acquire(path: &Path, on_wait: impl FnOnce()) -> io::Result<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                on_wait();
                file.lock()?;
            }
            Err(TryLockError::Error(err)) => return Err(err),
        }
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;

    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn unit_ci_lock_second_holder_waits_until_first_releases() {
        let dir = TempDir::new("lock-serial");
        let path = dir.path().join("machine.lock");
        let (events, seen) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();

        let first = thread::spawn({
            let (path, events) = (path.clone(), events.clone());
            move || {
                let _lock = MachineLock::acquire(&path, || panic!("nobody holds the lock yet"))
                    .expect("first acquire");
                events.send("first: running").expect("send");
                released.recv().expect("release signal");
                events.send("first: done").expect("send");
            }
        });
        assert_eq!(seen.recv().expect("event"), "first: running");

        let second = thread::spawn({
            let events = events.clone();
            move || {
                let waiting = events.clone();
                let _lock = MachineLock::acquire(&path, move || {
                    waiting.send("second: waiting").expect("send");
                })
                .expect("second acquire");
                events.send("second: running").expect("send");
            }
        });
        // The second job saw the lock taken and is blocked, not running.
        assert_eq!(seen.recv().expect("event"), "second: waiting");

        release.send(()).expect("release");
        first.join().expect("first job");
        second.join().expect("second job");
        drop(events);
        let rest: Vec<_> = seen.iter().collect();
        assert_eq!(rest, ["first: done", "second: running"]);
    }

    const HOLDER_ENV: &str = "XTASK_TEST_LOCK_HOLDER";
    const HOLDING: &str = "xtask-test-lock-holder-ready";

    #[test]
    fn unit_ci_lock_released_when_holder_is_killed() {
        if let Some(path) = std::env::var_os(HOLDER_ENV) {
            // Child process: hold the lock until the parent kills us.
            let _lock = MachineLock::acquire(Path::new(&path), || {}).expect("child acquire");
            // libtest has already printed `test <name> ... ` without a newline.
            println!("\n{HOLDING}");
            io::stdout().flush().expect("flush");
            let _ = io::stdin().read(&mut [0u8; 1]);
            return;
        }

        let dir = TempDir::new("lock-killed");
        let path = dir.path().join("machine.lock");
        let module = module_path!().split_once("::").map_or("", |(_, rest)| rest);
        let mut child = Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", "--nocapture", "--test-threads=1"])
            .arg(format!(
                "{module}::unit_ci_lock_released_when_holder_is_killed"
            ))
            .env(HOLDER_ENV, &path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn lock holder");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        assert!(
            stdout
                .lines()
                .any(|line| line.is_ok_and(|line| line == HOLDING)),
            "the child never took the lock"
        );

        let probe = OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open lock");
        assert!(matches!(probe.try_lock(), Err(TryLockError::WouldBlock)));

        child.kill().expect("kill holder");
        child.wait().expect("reap holder");
        let mut waited = false;
        let _lock = MachineLock::acquire(&path, || waited = true).expect("acquire after kill");
        assert!(!waited, "the lock outlived its killed holder");
    }
}
