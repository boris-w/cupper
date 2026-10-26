//! Machine-wide lock for the jobs that are sensitive to CPU contention: e2e, `ui`
//! and performance runs (plan 1.8). Several agents and the orchestrator run CI on
//! one machine; these jobs queue behind each other instead of skewing timings.
//!
//! The lock is an advisory `flock` on a file in the per-user temporary directory.
//! The kernel drops it when the holder's file descriptor closes, including when
//! the process is killed, so a crashed run never leaves a stale lock behind.
//!
//! `cargo xtask lock -- <command>` runs a command under the lock, such as the
//! e2e suite outside `xtask ci` (T093). What runs under the lock has
//! [`HELD_ENV`] set, and a `lock` that sees it runs its command at once: the
//! lock is already this run's, and taking it again would wait forever.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Set for what runs under the machine lock, to [`HELD`].
pub const HELD_ENV: &str = "CUPPER_MACHINE_LOCK";
pub const HELD: &str = "held";

pub const USAGE: &str = "usage: cargo xtask lock -- <command> [args...]

Runs the command holding the machine lock, and exits with its exit code.";

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

/// `cargo xtask lock -- <command>`.
pub fn main(args: &[String]) -> ExitCode {
    let command = args.strip_prefix(&["--".to_owned()]).unwrap_or(args);
    if command.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let held = std::env::var_os(HELD_ENV).is_some_and(|value| value == HELD);
    match run_locked(command, held, &MachineLock::default_path()) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(err) => {
            eprintln!("xtask lock: {err}");
            ExitCode::from(2)
        }
    }
}

/// Runs `command` holding the lock at `path`, unless `held`; its exit code,
/// or 128 plus the signal that ended it.
fn run_locked(command: &[String], held: bool, path: &Path) -> io::Result<i32> {
    let _lock = if held {
        None
    } else {
        Some(MachineLock::acquire(path, || {
            eprintln!("xtask lock: waiting for the machine lock held by another run");
        })?)
    };
    let (program, args) = command.split_first().ok_or(io::ErrorKind::InvalidInput)?;
    let status = Command::new(program)
        .args(args)
        .env(HELD_ENV, HELD)
        .status()?;
    Ok(status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::testutil::{TempDir, alone};

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

    fn sh(script: &str) -> [String; 3] {
        ["sh".to_owned(), "-c".to_owned(), script.to_owned()]
    }

    /// How long a released lock may still be held: after a process that held
    /// a copy of its fd was waited for, the lock is now and then free only a
    /// few ms later, in a test process of its own too (T093 fix 1,
    /// `notes-fix1.md`: 2 in about 20,000 checks, the longest 2.6 ms; none
    /// without such a process). Why is not known. In production the next
    /// waiter only gets the lock that much later.
    const RELEASE_LIMIT: Duration = Duration::from_secs(1);

    /// Fails unless the lock at `path` is free within [`RELEASE_LIMIT`].
    fn freed(path: &Path, after: &str) {
        let (done, freed) = mpsc::channel();
        let path = path.to_owned();
        thread::spawn(move || {
            let _ = done.send(MachineLock::acquire(&path, || {}).map(drop));
        });
        match freed.recv_timeout(RELEASE_LIMIT) {
            Ok(taken) => taken.expect("acquire"),
            Err(_) => panic!("the lock outlived {after} by more than {RELEASE_LIMIT:?}"),
        }
    }

    #[test]
    fn unit_ci_lock_command_passes_its_exit_code_and_is_told_it_holds_the_lock() {
        // On its own: another test's new process would hold a copy of the
        // lock's fd until it execs, and keep the lock past the command.
        if !alone(
            "ci::lock::tests::unit_ci_lock_command_passes_its_exit_code_and_is_told_it_holds_the_lock",
        ) {
            return;
        }
        let dir = TempDir::new("lock-run");
        let path = dir.path().join("machine.lock");
        assert_eq!(run_locked(&sh("exit 3"), false, &path).expect("run"), 3);
        let told = format!(r#"test "${HELD_ENV}" = {HELD}"#);
        assert_eq!(run_locked(&sh(&told), false, &path).expect("run"), 0);
        assert_eq!(
            run_locked(&sh("kill -TERM $$"), false, &path).expect("run"),
            128 + 15
        );
        // Released once the command is over, give or take a few ms (see
        // `RELEASE_LIMIT`).
        freed(&path, "its command");
    }

    #[test]
    fn unit_ci_lock_command_under_the_lock_does_not_wait_for_it() {
        let dir = TempDir::new("lock-held");
        let path = dir.path().join("machine.lock");
        let _held = MachineLock::acquire(&path, || {}).expect("hold");
        let (done, ran) = mpsc::channel();
        thread::spawn(move || {
            let code = run_locked(&sh("exit 7"), true, &path).expect("run");
            done.send(code).expect("send");
        });
        assert_eq!(ran.recv_timeout(Duration::from_secs(10)), Ok(7));
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
        freed(&path, "its killed holder");
    }
}
