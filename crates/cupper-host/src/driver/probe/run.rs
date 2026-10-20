//! A one-shot child process (design 5.6.6): started through
//! [`cupper_sys::spawn`] with stdin and stderr on `/dev/null`, its stdout
//! collected up to [`OUTPUT_LIMIT`]. The time limit runs on the host's
//! [`Clock`]; when it passes, every process group of the child's tree is
//! killed and the child reaped.

use std::io;
use std::os::fd::OwnedFd;
use std::pin::pin;
use std::process::ExitStatus;
use std::time::Duration;

use cupper_sys::{Command, Signal, Stdio};
use tokio::io::AsyncReadExt;
use tokio::net::unix::pipe;

use crate::clock::Clock;

/// More output than this fails the run; an environment or a version line is
/// far smaller.
pub const OUTPUT_LIMIT: usize = 1 << 20;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("cannot start the program")]
    Spawn(#[source] cupper_sys::Error),
    #[error("the program did not finish in time")]
    Timeout,
    #[error("the program wrote more than {OUTPUT_LIMIT} bytes")]
    TooMuchOutput,
    #[error("cannot read the program's output")]
    Read(#[source] io::Error),
    #[error("cannot wait for the program")]
    Wait,
}

#[derive(Debug)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
}

/// Runs `command` to its end, or kills it once `limit` has passed.
pub async fn run(
    clock: &dyn Clock,
    mut command: Command,
    limit: Duration,
) -> Result<Output, RunError> {
    command
        .stdin(Stdio::Null)
        .stdout(Stdio::Pipe)
        .stderr(Stdio::Null);
    let mut timer = pin!(clock.timer_at(clock.now_mono() + limit));
    let mut child = cupper_sys::spawn(&command).map_err(RunError::Spawn)?;
    let group = child.group();
    let stdout = child.stdout.take().expect("stdout is a pipe");
    let read = tokio::select! {
        read = read_capped(stdout) => read,
        () = &mut timer => Err(RunError::Timeout),
    };
    let mut waiter = tokio::task::spawn_blocking(move || child.wait());
    let status = match &read {
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
        return Err(read.err().unwrap_or(RunError::Timeout));
    };
    let status = status
        .map_err(|_| RunError::Wait)?
        .map_err(|_| RunError::Wait)?;
    Ok(Output {
        status,
        stdout: read?,
    })
}

async fn read_capped(stdout: OwnedFd) -> Result<Vec<u8>, RunError> {
    let reader = pipe::Receiver::from_owned_fd(stdout).map_err(RunError::Read)?;
    let mut out = Vec::new();
    // One byte past the limit tells "exactly the limit" from "more".
    let capped = OUTPUT_LIMIT as u64 + 1;
    reader
        .take(capped)
        .read_to_end(&mut out)
        .await
        .map_err(RunError::Read)?;
    if out.len() > OUTPUT_LIMIT {
        return Err(RunError::TooMuchOutput);
    }
    Ok(out)
}
