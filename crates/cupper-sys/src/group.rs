//! Process groups and their cleanup (design 5.6.8, 5.2.13).
//!
//! Signalling an adapter's process group is not enough: Claude Code runs every
//! tool command in a session and process group of its own, and those outlive
//! the adapter (T003). So, before any signal, record the groups of the
//! adapter's whole process tree ([`ProcessGroup::with_descendants`]), then
//! signal each recorded group. Waiting between SIGTERM and SIGKILL is up to
//! the caller and its `Clock`.

use std::io;

use crate::proc::{ProcessInfo, StartTime, descendants, group_members, process_info};
use crate::spawn::Child;
use crate::{Error, Result};

/// A process group as recorded: its id and the start time of its leader, or
/// of its oldest member if the leader had already exited. The record can
/// outlive its processes (the host keeps it in the `adapters` table) and keeps
/// a later group that reuses the number from being signalled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProcessGroup {
    pub pgid: i32,
    pub start: StartTime,
}

/// The signals cleanup sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Term,
    Kill,
}

impl Signal {
    fn raw(self) -> libc::c_int {
        match self {
            Self::Term => libc::SIGTERM,
            Self::Kill => libc::SIGKILL,
        }
    }
}

impl Child {
    /// The group the child leads (pgid = pid), recorded at spawn.
    pub fn group(&self) -> ProcessGroup {
        ProcessGroup {
            pgid: self.pid(),
            start: self.start(),
        }
    }
}

impl ProcessGroup {
    /// Records group `pgid` as it is now; `None` when it has no live member.
    pub(crate) fn capture(pgid: i32) -> Option<Self> {
        let members = group_members(pgid);
        let leader = members.iter().find(|m| m.pid == pgid);
        let start = leader
            .or_else(|| members.iter().min_by_key(|m| m.start))?
            .start;
        Some(Self { pgid, start })
    }

    /// The live members, or none when the group is gone or is not the
    /// recorded one any more. A group id is only reused once all members of
    /// the old group have exited, so the current group is the recorded one
    /// when its leader, if alive, has the recorded start time and no member
    /// started before it. (Left unnoticed: the number was reused, and the new
    /// leader exited while its group lives on.)
    pub fn members(&self) -> Vec<ProcessInfo> {
        let members = group_members(self.pgid);
        let recorded = members
            .iter()
            .all(|m| m.start >= self.start && (m.pid != self.pgid || m.start == self.start));
        if recorded { members } else { Vec::new() }
    }

    /// Whether the recorded group still has a live member.
    pub fn is_alive(&self) -> bool {
        !self.members().is_empty()
    }

    /// Sends `signal` to every member if the group is still the recorded one.
    /// `Ok(false)` when it is gone or is another group now. Our own group and
    /// the ids 0 and 1 are refused.
    pub fn signal(&self, signal: Signal) -> Result<bool> {
        signallable(self.pgid)?;
        if !self.is_alive() {
            return Ok(false);
        }
        // SAFETY: killpg takes plain integers; `pgid` is above 1 and not our
        // own group. Between the check above and this call the group could
        // only change if all its members exited first (see `members`).
        if unsafe { libc::killpg(self.pgid, signal.raw()) } == 0 {
            return Ok(true);
        }
        let source = io::Error::last_os_error();
        if source.raw_os_error() == Some(libc::ESRCH) {
            return Ok(false);
        }
        Err(Error::Os {
            call: "killpg",
            source,
        })
    }

    /// This group first, then the groups of all live descendants of its
    /// leader, each recorded now. Take it before sending any signal: once the
    /// leader exits, its descendants belong to launchd and cannot be found
    /// from it any more (design 5.6.8). When the leader is gone, or the group
    /// is not the recorded one, only this group is returned. Our own group is
    /// never included.
    pub fn with_descendants(&self) -> Vec<ProcessGroup> {
        let mut groups = vec![*self];
        let leader_is_ours =
            process_info(self.pgid).is_some_and(|leader| leader.start == self.start);
        if !leader_is_ours {
            return groups;
        }
        for process in descendants(self.pgid) {
            let pgid = process.pgid;
            if signallable(pgid).is_err() || groups.iter().any(|g| g.pgid == pgid) {
                continue;
            }
            if let Some(group) = Self::capture(pgid) {
                groups.push(group);
            }
        }
        groups
    }
}

/// `killpg(0)` signals our own group and `killpg(1)` launchd's; cleanup never
/// targets either, nor our own group by number.
fn signallable(pgid: i32) -> Result<()> {
    // SAFETY: getpgrp has no preconditions and cannot fail.
    let own = unsafe { libc::getpgrp() };
    if pgid <= 1 || pgid == own {
        return Err(Error::InvalidArgument(
            "process group 0, 1 or our own is never signalled",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::os::unix::process::ExitStatusExt;
    use std::time::Duration;

    use super::*;
    use crate::testutil::{Groups, Owned, cat, eventually, sh};

    const GONE: Duration = Duration::from_secs(5);

    /// Starts `sh -c script` in a session of its own, like an adapter, and
    /// returns it with the first line it prints.
    fn adapter(script: &str) -> (Owned, String) {
        let mut child = Owned::spawn(sh(script).new_session());
        let mut line = String::new();
        BufReader::new(child.stdout_file())
            .read_line(&mut line)
            .unwrap();
        (child, line.trim().to_owned())
    }

    fn exited(process: &ProcessInfo) -> bool {
        process_info(process.pid).is_none_or(|now| now.start != process.start)
    }

    #[test]
    fn unit_sys_reap_group_signals_the_whole_group() {
        let (mut child, ready) = adapter("/bin/sleep 30 & /bin/sleep 30 & echo ready; wait");
        assert_eq!(ready, "ready");
        let group = child.group();
        let members = group.members();
        assert_eq!(members.len(), 3, "{members:?}");
        assert_eq!(group.with_descendants(), [group]);

        assert!(group.signal(Signal::Term).unwrap());
        assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGTERM));
        assert!(eventually(GONE, || !group.is_alive()));
        assert!(members.iter().all(exited), "{members:?}");
        assert!(!group.signal(Signal::Kill).unwrap());
    }

    #[test]
    fn unit_sys_reap_group_reaches_descendants_in_other_groups() {
        // Like a Claude Code tool command (T003): the adapter's child runs in
        // a process group of its own (`set -m` gives each job its own group).
        let (mut child, tool_pid) = adapter("set -m; /bin/sleep 30 & echo $!; wait");
        let tool = process_info(tool_pid.parse().unwrap()).unwrap();
        assert_ne!(tool.pgid, child.pid());
        assert_eq!(tool.ppid, child.pid());

        // Recorded before any signal, as design 5.6.8 requires.
        let groups = child.group().with_descendants();
        let _cleanup = Groups(groups.clone());
        assert_eq!(groups.len(), 2, "{groups:?}");
        assert_eq!(groups[1].pgid, tool.pgid);
        let found: Vec<(i32, i32)> = descendants(child.pid())
            .iter()
            .map(|d| (d.pid, d.pgid))
            .collect();
        assert_eq!(found, [(tool.pid, tool.pgid)]);

        // Signalling only the adapter's group leaves the tool command running,
        // adopted by launchd, where the adapter's tree no longer shows it.
        assert!(groups[0].signal(Signal::Kill).unwrap());
        assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGKILL));
        assert!(eventually(GONE, || {
            process_info(tool.pid).is_some_and(|now| now.ppid == 1)
        }));

        // The groups recorded before still reach it.
        let signalled: Vec<bool> = groups
            .iter()
            .map(|g| g.signal(Signal::Term).unwrap())
            .collect();
        assert_eq!(signalled, [false, true]);
        assert!(eventually(GONE, || exited(&tool)));
        assert!(!groups[1].is_alive());
    }

    #[test]
    fn unit_sys_reap_group_leaves_other_groups_with_the_same_id_alone() {
        let mut child = Owned::spawn(&cat());
        let recorded = child.group();
        let micros = recorded.start.as_micros();
        // A record older than the live leader, and one newer than every live
        // member: both describe another group that had the same id.
        for start in [micros - 1, micros + 1] {
            let other = ProcessGroup {
                pgid: recorded.pgid,
                start: StartTime::from_micros(start),
            };
            assert!(other.members().is_empty());
            assert!(!other.signal(Signal::Kill).unwrap());
            assert_eq!(other.with_descendants(), [other]);
        }
        assert!(process_info(child.pid()).is_some());

        assert!(recorded.signal(Signal::Term).unwrap());
        assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGTERM));
    }

    #[test]
    fn unit_sys_reap_group_never_targets_our_group_or_launchd() {
        // SAFETY: getpgrp has no preconditions.
        let own = unsafe { libc::getpgrp() };
        for pgid in [own, 1, 0, -1] {
            assert!(
                matches!(signallable(pgid), Err(Error::InvalidArgument(_))),
                "{pgid}"
            );
        }
        assert!(signallable(own + 1).is_ok() || own + 1 == 1);
    }

    #[test]
    fn unit_sys_reap_group_capture_falls_back_to_the_oldest_member() {
        // The leader `sh` exits and leaves its two `sleep`s in the group.
        let (mut child, first) = adapter("/bin/sleep 30 & echo $!; /bin/sleep 30 & exit 0");
        let pgid = child.pid();
        let _cleanup = Groups(vec![child.group()]);
        let first = process_info(first.parse().unwrap()).unwrap();
        assert!(child.wait().unwrap().success());
        let members = group_members(pgid);
        assert_eq!(members.len(), 2, "{members:?}");
        assert!(members.iter().all(|m| m.pid != pgid));

        let captured = ProcessGroup::capture(pgid).unwrap();
        assert_eq!(captured.start, first.start);
        let pids = |list: Vec<ProcessInfo>| {
            let mut pids: Vec<i32> = list.iter().map(|m| m.pid).collect();
            pids.sort_unstable();
            pids
        };
        assert_eq!(pids(captured.members()), pids(members));
        assert!(captured.signal(Signal::Kill).unwrap());
        assert!(eventually(GONE, || !captured.is_alive()));
    }
}
