//! Running one git command (design 5.2.10): started through
//! [`cupper_sys::spawn`], given `input` on stdin (`apply` reads its patch
//! there) or `/dev/null`, its stdout and stderr collected separately. The
//! time limit runs on the host's [`Clock`]; when it passes, every process
//! group of the child's tree is killed and the child reaped.

use std::io;
use std::os::fd::OwnedFd;
use std::pin::pin;
use std::process::ExitStatus;
use std::time::Duration;

use cupper_sys::{Command, Signal, Stdio};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::unix::pipe;

use crate::clock::Clock;

/// More stdout than this fails the run.
pub const STDOUT_LIMIT: usize = 64 << 20;
/// Only the start of stderr is kept; the rest is read and dropped.
pub const STDERR_KEPT: usize = 64 << 10;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("cannot start git")]
    Spawn(#[source] cupper_sys::Error),
    #[error("git did not finish in time")]
    Timeout,
    #[error("git wrote more than {STDOUT_LIMIT} bytes")]
    TooMuchOutput,
    #[error("cannot talk to git")]
    Io(#[source] io::Error),
    #[error("cannot wait for git")]
    Wait,
}

#[derive(Debug)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    /// At most [`STDERR_KEPT`] bytes.
    pub stderr: Vec<u8>,
}

/// Runs `command` to its end, or kills it once `limit` has passed.
pub async fn run(
    clock: &dyn Clock,
    mut command: Command,
    input: Option<Vec<u8>>,
    limit: Duration,
) -> Result<Output, RunError> {
    let stdin = input.as_ref().map_or(Stdio::Null, |_| Stdio::Pipe);
    command.stdin(stdin).stdout(Stdio::Pipe).stderr(Stdio::Pipe);
    let mut timer = pin!(clock.timer_at(clock.now_mono() + limit));
    let mut child = cupper_sys::spawn(&command).map_err(RunError::Spawn)?;
    let group = child.group();
    let stdin = child.stdin.take();
    let stdout = child.stdout.take().expect("stdout is a pipe");
    let stderr = child.stderr.take().expect("stderr is a pipe");
    let talk = async {
        let (written, stdout, stderr) = tokio::join!(
            write(stdin, input),
            read(stdout, STDOUT_LIMIT, false),
            read(stderr, STDERR_KEPT, true),
        );
        written?;
        Ok((stdout?, stderr?))
    };
    let talked = tokio::select! {
        talked = talk => talked,
        () = &mut timer => Err(RunError::Timeout),
    };
    let mut waiter = tokio::task::spawn_blocking(move || child.wait());
    let status = match &talked {
        Ok(_) => tokio::select! {
            status = &mut waiter => Some(status),
            () = &mut timer => None,
        },
        Err(_) => None,
    };
    let Some(status) = status else {
        // Groups first: once the child is gone its descendants cannot be found.
        for group in group.with_descendants() {
            let _ = group.signal(Signal::Kill);
        }
        let _ = waiter.await;
        return Err(talked.err().unwrap_or(RunError::Timeout));
    };
    let status = status
        .map_err(|_| RunError::Wait)?
        .map_err(|_| RunError::Wait)?;
    let (stdout, stderr) = talked?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// Writes `input` and closes stdin. A git that exits without reading all of
/// it is not an error here: its exit status tells.
async fn write(stdin: Option<OwnedFd>, input: Option<Vec<u8>>) -> Result<(), RunError> {
    let (Some(stdin), Some(input)) = (stdin, input) else {
        return Ok(());
    };
    let mut sender = pipe::Sender::from_owned_fd(stdin).map_err(RunError::Io)?;
    match sender.write_all(&input).await {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => Err(RunError::Io(err)),
        _ => Ok(()),
    }
}

/// Reads to the end, keeping at most `limit` bytes; past it, either fails
/// with [`RunError::TooMuchOutput`] or, when `drop_rest`, drops the rest.
async fn read(fd: OwnedFd, limit: usize, drop_rest: bool) -> Result<Vec<u8>, RunError> {
    let mut reader = pipe::Receiver::from_owned_fd(fd).map_err(RunError::Io)?;
    let mut kept = Vec::new();
    let mut chunk = vec![0; 64 << 10];
    loop {
        let n = reader.read(&mut chunk).await.map_err(RunError::Io)?;
        if n == 0 {
            return Ok(kept);
        }
        let room = limit - kept.len();
        if n > room && !drop_rest {
            return Err(RunError::TooMuchOutput);
        }
        kept.extend_from_slice(&chunk[..n.min(room)]);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::clock::ManualClock;
    use crate::driver::probe::testkit::script;
    use crate::store::testutil::TempDir;

    const LIMIT: Duration = Duration::from_secs(30);

    #[tokio::test]
    async fn unit_git_cmd_run_feeds_stdin_and_keeps_stdout_and_stderr_apart() {
        let dir = TempDir::new("git-run");
        let program = script(dir.path(), "git", "cat; echo oops >&2; exit 3");
        let clock = ManualClock::new();
        let input = b"diff --git a/x b/x\n".to_vec();
        let got = run(&clock, Command::new(&program), Some(input.clone()), LIMIT);
        let got = got.await.expect("runs");
        assert_eq!(
            (got.status.code(), got.stdout, got.stderr),
            (Some(3), input, b"oops\n".to_vec())
        );
        // Without input, stdin is `/dev/null`: `cat` ends at once.
        let got = run(&clock, Command::new(&program), None, LIMIT)
            .await
            .expect("runs");
        assert!(got.stdout.is_empty());
    }

    #[tokio::test]
    async fn unit_git_cmd_run_kills_git_at_the_timeout() {
        let dir = TempDir::new("git-run-hang");
        let pid_file = dir.path().join("pid");
        let body = format!("echo $$ > {}\nexec tail -f /dev/null", pid_file.display());
        let program = script(dir.path(), "git", &body);
        let clock = Arc::new(ManualClock::new());
        let running = tokio::spawn({
            let clock = Arc::clone(&clock);
            async move { run(&*clock, Command::new(&program), None, LIMIT).await }
        });
        let started = || std::fs::read_to_string(&pid_file).is_ok_and(|pid| pid.ends_with('\n'));
        while clock.next_deadline().is_none() || !started() {
            tokio::task::yield_now().await;
        }
        clock.advance(LIMIT);
        let result = running.await.expect("the run ends");
        assert!(matches!(result, Err(RunError::Timeout)), "{result:?}");
        let pid = std::fs::read_to_string(&pid_file).expect("pid");
        let pid: i32 = pid.trim().parse().expect("a pid");
        assert!(cupper_sys::process_info(pid).is_none(), "git was killed");
    }
}
