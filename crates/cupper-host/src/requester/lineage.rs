//! The lineage signals (design 5.8.4): whether a process that asks for
//! elevation could be an adapter's descendant. Pure: the caller reads the
//! process table and the `adapters` table.
//!
//! Adapters lead a session and a process group of their own (5.8.5). A
//! requester is a descendant when it is in an adapter's session or group,
//! started after the adapter, or when the adapter is among its ancestors. For
//! Claude's tool commands, which the CLI starts in sessions of their own
//! (T003), only the ancestors tell; MCP servers and hooks stay in the
//! adapter's group. A command detached through `nohup`, a double fork or
//! `disown` escapes all three: a known limit, the user's confirmation being
//! the defence.

use cupper_sys::{ProcessInfo, StartTime};

/// An `adapters` row that has not ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveAdapter {
    pub pid: i32,
    pub pgid: i32,
    pub start: StartTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    SameSession,
    SameGroup,
    Ancestor,
}

impl Signal {
    /// As the audit record names it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SameSession => "same_session",
            Self::SameGroup => "same_group",
            Self::Ancestor => "ancestor",
        }
    }
}

/// The signals that `chain`, the requester and then its ancestors nearest
/// first, gives against `adapters`. `now` holds the processes that run with
/// the adapters' pids now: a row whose pid runs a process of another start
/// time is stale (the pid was reused) and gives nothing. A row whose pid runs
/// nothing still counts for its session and group, which outlive their
/// leader. An empty chain gives nothing: the caller cannot rule anything out.
pub fn signals(
    chain: &[ProcessInfo],
    adapters: &[LiveAdapter],
    now: &[ProcessInfo],
) -> Vec<Signal> {
    let Some((me, ancestors)) = chain.split_first() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let reused = |adapter: &LiveAdapter| {
        now.iter()
            .any(|p| p.pid == adapter.pid && p.start != adapter.start)
    };
    for adapter in adapters.iter().filter(|adapter| !reused(adapter)) {
        let after = me.start >= adapter.start;
        let is = |p: &ProcessInfo| p.pid == adapter.pid && p.start == adapter.start;
        let matched = [
            (Signal::SameSession, after && me.sid == adapter.pid),
            (Signal::SameGroup, after && me.pgid == adapter.pgid),
            (Signal::Ancestor, ancestors.iter().any(is)),
        ];
        for (signal, _) in matched.into_iter().filter(|(_, hit)| *hit) {
            if !found.contains(&signal) {
                found.push(signal);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Process `pid` of group `pgid` in session `sid`, child of `ppid`,
    /// started at `start`.
    fn process(pid: i32, ppid: i32, pgid: i32, sid: i32, start: u64) -> ProcessInfo {
        let (uid, start, name) = (501, StartTime::from_micros(start), String::new());
        ProcessInfo {
            pid,
            ppid,
            pgid,
            sid,
            uid,
            start,
            name,
        }
    }

    #[test]
    fn unit_lineage_signals_each_signal_reuse_and_detached_commands() {
        use Signal::*;
        let adapter = LiveAdapter {
            pid: 100,
            pgid: 100,
            start: StartTime::from_micros(10),
        };
        let launchd = process(1, 0, 1, 1, 0);
        let running = [process(100, 1, 100, 100, 10)];
        // A Claude tool command: the CLI made it a session of its own.
        let tool = [
            process(300, 200, 300, 300, 30),
            process(200, 100, 100, 100, 20),
            running[0].clone(),
            launchd.clone(),
        ];
        // An MCP server or a hook: in the adapter's session and group.
        let mcp = [
            process(210, 200, 100, 100, 25),
            tool[1].clone(),
            running[0].clone(),
        ];
        // Detached by a double fork: launchd adopted it.
        let detached = [process(310, 1, 310, 310, 31), launchd.clone()];
        // A terminal's CLI, started before the adapter, in no adapter's tree.
        let terminal = [
            process(50, 40, 50, 40, 5),
            process(40, 1, 40, 40, 4),
            launchd,
        ];
        let cases: [(&[ProcessInfo], &[ProcessInfo], &[Signal]); 6] = [
            (&tool, &running, &[Ancestor]),
            (&mcp, &running, &[SameSession, SameGroup, Ancestor]),
            (&detached, &running, &[]),
            (&terminal, &running, &[]),
            // The adapter died and its pid went to another process: stale.
            (&mcp, &[process(100, 1, 100, 100, 99)], &[]),
            // The adapter died, its session lives on in what it left.
            (&mcp[..1], &[], &[SameSession, SameGroup]),
        ];
        for (n, (chain, now, expected)) in cases.into_iter().enumerate() {
            assert_eq!(signals(chain, &[adapter], now), expected, "case {n}");
        }
        assert_eq!(signals(&[], &[adapter], &running), []);
        // An ancestor with the adapter's pid but another start is not it.
        let other = [tool[0].clone(), process(100, 1, 100, 100, 11)];
        assert_eq!(signals(&other, &[adapter], &[]), []);
    }
}
