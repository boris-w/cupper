//! The exit status of a process that is not our child (IT-devkey-orphan-start):
//! `waitpid` only reaps children, but a kqueue `EVFILT_PROC` filter with
//! `NOTE_EXIT | NOTE_EXITSTATUS` reports the status of any process of this
//! user, one that launchd has adopted included (macOS 26.4).

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::time::Duration;
use std::{io, ptr};

use crate::{Error, Result};

/// A watch on the exit of one process. It has to be set while the process
/// runs: one that has exited cannot be watched any more.
#[derive(Debug)]
pub struct ExitWatch {
    queue: OwnedFd,
    pid: i32,
}

impl ExitWatch {
    /// Watches `pid`; fails with `ESRCH` when there is no such process left.
    pub fn new(pid: i32) -> Result<Self> {
        if pid <= 0 {
            return Err(Error::InvalidArgument("pid"));
        }
        // SAFETY: kqueue takes no arguments.
        let fd = unsafe { libc::kqueue() };
        if fd < 0 {
            return Err(Error::last_os("kqueue"));
        }
        // SAFETY: `fd` is a new descriptor that nothing else owns.
        let queue = unsafe { OwnedFd::from_raw_fd(fd) };
        let change = exit_event(pid);
        // SAFETY: one valid change in, no room for events out, so errors come
        // back through the return value; no timeout pointer.
        let rc = unsafe {
            libc::kevent(
                queue.as_raw_fd(),
                &change,
                1,
                ptr::null_mut(),
                0,
                ptr::null(),
            )
        };
        if rc < 0 {
            return Err(Error::last_os("kevent(EVFILT_PROC)"));
        }
        Ok(Self { queue, pid })
    }

    /// The exit status, once the process exits within `timeout`; `None` if
    /// it still runs then. A signal restarts the wait.
    pub fn wait(&self, timeout: Duration) -> Result<Option<ExitStatus>> {
        let limit = libc::timespec {
            tv_sec: libc::time_t::try_from(timeout.as_secs()).unwrap_or(libc::time_t::MAX),
            tv_nsec: libc::c_long::from(timeout.subsec_nanos()),
        };
        let mut got = exit_event(self.pid);
        loop {
            // SAFETY: no changes in; room for one event out; `limit` and `got`
            // outlive the call.
            let rc = unsafe {
                libc::kevent(self.queue.as_raw_fd(), ptr::null(), 0, &mut got, 1, &limit)
            };
            match rc {
                0 => return Ok(None),
                1 if got.fflags & libc::NOTE_EXITSTATUS != 0 => {
                    let status =
                        i32::try_from(got.data).map_err(|_| Error::Unexpected("kevent"))?;
                    return Ok(Some(ExitStatus::from_raw(status)));
                }
                1 => return Err(Error::Unexpected("kevent")),
                _ if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted => {}
                _ => return Err(Error::last_os("kevent")),
            }
        }
    }
}

fn exit_event(pid: i32) -> libc::kevent {
    libc::kevent {
        ident: pid as usize,
        filter: libc::EVFILT_PROC,
        flags: libc::EV_ADD | libc::EV_ONESHOT,
        fflags: libc::NOTE_EXIT | libc::NOTE_EXITSTATUS,
        data: 0,
        udata: ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};

    use super::*;
    use crate::process_info;
    use crate::testutil::{Owned, sh};

    #[test]
    fn exit_watch_reports_the_status_of_an_adopted_process() {
        // sh starts a subshell that waits for a line from us, then exits.
        let mut cmd = sh("exec 3<&0; (read x <&3; exit 7) & echo $!");
        cmd.stdin(crate::Stdio::Pipe);
        let mut shell = Owned::spawn(&cmd);
        let mut go = shell.stdin_file();
        let mut line = String::new();
        BufReader::new(shell.stdout_file())
            .read_line(&mut line)
            .unwrap();
        let orphan: i32 = line.trim().parse().unwrap();
        assert!(shell.wait().unwrap().success());
        assert_eq!(process_info(orphan).unwrap().ppid, 1, "adopted by launchd");

        let watch = ExitWatch::new(orphan).unwrap();
        assert_eq!(watch.wait(Duration::ZERO).unwrap(), None, "still running");
        go.write_all(b"go\n").unwrap();
        let status = watch.wait(Duration::from_secs(10)).unwrap().unwrap();
        assert_eq!(status.code(), Some(7));
    }

    #[test]
    fn exit_watch_needs_a_live_process() {
        let err = ExitWatch::new(i32::MAX).unwrap_err();
        assert!(
            matches!(&err, Error::Os { source, .. } if source.raw_os_error() == Some(libc::ESRCH)),
            "{err:?}"
        );
        assert!(matches!(ExitWatch::new(0), Err(Error::InvalidArgument(_))));
    }
}
