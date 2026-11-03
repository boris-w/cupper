//! `git/diff` in the snapshot tier (design 5.2.10, "diff 与 hunk"). Every
//! range compares two trees: a turn's start and end snapshots, or its start
//! and a snapshot taken now while it runs; the index and a snapshot taken
//! now (`unstaged`); HEAD and the index (`staged`). Trees never change, so
//! the cache keeps a diff as its trees and file list: a file's patch is the
//! same whenever it is made from them.

mod cache;

use std::collections::BTreeSet;
use std::path::Path;

use cupper_protocol::annotations::WirePath;
use cupper_protocol::ids::TurnId;
use cupper_protocol::item::{DiffChange, Item};
use cupper_protocol::methods::git::{DiffSnapshot, GitDiffFile};
use cupper_protocol::session::{DiffStat, ExcludedPath, SnapshotStatus};
use cupper_protocol::view::{Entry, ItemView, SessionView};

pub use cache::DiffCache;

use super::snapshot::{self, Opened, ShotError, Taken, output, shoot};
use super::stamp::Fingerprint;
use super::{GitCmd, GitTool, SnapshotOp};
use crate::clock::Clock;
use crate::durations::Durations;

/// What a diff compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Range {
    Unstaged,
    /// HEAD's commit; `None` before the first one.
    Staged(Option<String>),
    /// A turn's start tree and end tree, `None` while it runs.
    Turn(String, Option<String>),
}

/// A diff made.
#[derive(Debug, Clone)]
pub struct Diffed {
    pub old: String,
    pub new: String,
    pub files: Vec<GitDiffFile>,
    /// The snapshot a running turn's range took now.
    pub taken: Option<Taken>,
}

/// The diff of `range` in `work_tree`. `unstaged` and `staged` fail with
/// [`ShotError::Unmerged`] while the index has a conflict.
pub async fn diff(
    tool: &GitTool,
    work_tree: &Path,
    range: Range,
    clock: &dyn Clock,
    durations: &Durations,
) -> Result<Diffed, ShotError> {
    let opened = snapshot::open(tool, work_tree, clock, durations).await?;
    let made = make(tool, &opened, range, clock, durations).await;
    opened.close().await;
    made
}

async fn make(
    tool: &GitTool,
    opened: &Opened,
    range: Range,
    clock: &dyn Clock,
    durations: &Durations,
) -> Result<Diffed, ShotError> {
    let tier = tool.snapshot_tier(&opened.place);
    let out = |cmd, what| output(&tier, cmd, what, clock, durations);
    let now = || {
        shoot(
            &tier,
            &opened.unrun,
            None,
            clock,
            durations,
            Fingerprint::default(),
        )
    };
    let (old, new, taken) = match range {
        Range::Turn(start, Some(end)) => (start, end, None),
        Range::Turn(start, None) => {
            let taken = now().await?;
            (start, taken.tree.clone(), Some(taken))
        }
        Range::Unstaged | Range::Staged(_) => {
            if !out(GitCmd::snapshot(SnapshotOp::Unmerged), "ls-files")
                .await?
                .is_empty()
            {
                return Err(ShotError::Unmerged);
            }
            let index = out(GitCmd::snapshot(SnapshotOp::WriteTree), "write-tree").await?;
            let index = String::from_utf8_lossy(index.trim_ascii()).into_owned();
            match range {
                Range::Staged(head) => (head.unwrap_or(opened.empty_tree().into()), index, None),
                _ => (index, now().await?.tree, None),
            }
        }
    };
    let list = GitCmd::snapshot(SnapshotOp::DiffList).args([&old, &new]);
    let mut files = files(&out(list, "diff-tree").await?);
    if !opened.unrun.is_empty() && !files.is_empty() {
        let paths = files
            .iter()
            .flat_map(|f| [f.path.0.as_bytes(), b"\0"].concat());
        let check = GitCmd::snapshot(SnapshotOp::CheckAttr).input(paths.collect());
        let attrs = out(check, "check-attr").await?;
        let hits = snapshot::filtered(&attrs, &opened.unrun).into_iter();
        let hits: BTreeSet<Vec<u8>> = hits.map(|(path, _)| path).collect();
        for file in &mut files {
            file.filter_unsupported = hits.contains(file.path.0.as_bytes());
        }
    }
    Ok(Diffed {
        old,
        new,
        files,
        taken,
    })
}

/// `diff-tree -z --raw --numstat` output: the raw records, then a numstat
/// record for each, in the same order. A rename's are `R<score>` with both
/// paths, and a numstat record with an empty path followed by both.
fn files(output: &[u8]) -> Vec<GitDiffFile> {
    let mut fields = output.split(|b| *b == 0);
    let path =
        |field: Option<&[u8]>| WirePath(String::from_utf8_lossy(field.unwrap_or_default()).into());
    let mut raw = Vec::new();
    let mut field = fields.next();
    while let Some(meta) = field.and_then(|f| f.strip_prefix(b":")) {
        let status = meta.rsplit(|b| *b == b' ').next().unwrap_or_default();
        let first = path(fields.next());
        let (path, old_path) = match status.first() {
            Some(b'R' | b'C') => (path(fields.next()), Some(first)),
            _ => (first, None),
        };
        let change = match status.first() {
            Some(b'A') => DiffChange::Add,
            Some(b'D') => DiffChange::Delete,
            _ => DiffChange::Update,
        };
        raw.push((path, old_path, change));
        field = fields.next();
    }
    let count = |n: Option<&[u8]>| std::str::from_utf8(n?).ok()?.parse::<u32>().ok();
    let mut listed = Vec::new();
    for (path, old_path, change) in raw {
        let mut stat = field.unwrap_or_default().splitn(3, |b| *b == b'\t');
        let (additions, deletions) = (count(stat.next()), count(stat.next()));
        if stat.next().is_some_and(<[u8]>::is_empty) {
            fields.nth(1);
        }
        field = fields.next();
        listed.push(GitDiffFile {
            path,
            old_path,
            change,
            // `-` for a binary file.
            binary: additions.is_none(),
            additions: additions.unwrap_or(0),
            deletions: deletions.unwrap_or(0),
            filter_unsupported: false,
        });
    }
    listed
}

/// The lines `files` add and remove.
pub fn stat(files: &[GitDiffFile]) -> DiffStat {
    DiffStat {
        files: u32::try_from(files.len()).unwrap_or(u32::MAX),
        added: files.iter().map(|f| f.additions).sum(),
        removed: files.iter().map(|f| f.deletions).sum(),
    }
}

/// What a turn range notes, from the session's view: its snapshots, with
/// what they left out, and whether commands that may still run could have
/// changed the work tree meanwhile (design 5.2.6, "晚到的工具输出"): the turn
/// has `late_tool_output` items, or a command that may still run ended
/// before the turn did and had not finished by the time it started.
pub fn turn_notes(view: &SessionView, turn: &TurnId) -> Option<(DiffSnapshot, bool)> {
    let at = view.turns.iter().find(|t| t.started.turn_id == *turn)?;
    let mut snapshot = DiffSnapshot {
        status: SnapshotStatus::Ok,
        excluded: Vec::new(),
    };
    for shot in &at.snapshots {
        add(
            &mut snapshot,
            shot.status != SnapshotStatus::Ok,
            &shot.excluded,
        );
    }
    let items = at.entries.iter().filter_map(|entry| match entry {
        Entry::Item(id) => view.items.get(id),
        Entry::Request(_) => None,
    });
    let late = items
        .into_iter()
        .any(|i| matches!(i.item.item, Item::LateToolOutput(_)));
    let (started, ended) = (
        at.started.started_at,
        at.completed.as_ref().map(|c| c.ended_at),
    );
    let running = |id, item: &ItemView| {
        let Some(end) = item.end.as_ref().filter(|end| end.may_still_run) else {
            return false;
        };
        let late = view.late_by_ref.get(id).and_then(|l| view.items.get(l));
        let finished = late.and_then(|l| l.end.as_ref()).map(|e| e.ended_at);
        ended.is_none_or(|ended| end.ended_at <= ended) && finished.is_none_or(|f| f >= started)
    };
    let still = view.items.iter().any(|(id, item)| running(id, item));
    Some((snapshot, late || still))
}

/// Adds to `snapshot` what another snapshot left out, `partial` or not.
pub fn add(snapshot: &mut DiffSnapshot, partial: bool, excluded: &[ExcludedPath]) {
    if partial || !excluded.is_empty() {
        snapshot.status = SnapshotStatus::Partial;
    }
    let new: Vec<_> = excluded
        .iter()
        .filter(|e| !snapshot.excluded.contains(e))
        .collect();
    snapshot.excluded.extend(new.into_iter().cloned());
}

#[cfg(test)]
mod tests {
    use cupper_protocol::view::View;
    use serde_json::json;

    use super::*;
    use crate::store::testutil::body;

    /// Turn 1 cancels a command that may still run; its output comes late in
    /// turn 2 and ends there; turn 3 starts after that.
    #[test]
    fn unit_git_diff_turn_notes_say_what_snapshots_left_and_late_commands() {
        let too_large = json!([{"path": "big.bin", "reason": "too_large"}]);
        let events = json!([
            ["turn.started", {"turn_id": "t_1", "turn_no": 1, "origin": "user", "started_at": 1}],
            ["snapshot.recorded", {"turn_id": "t_1", "phase": "start", "status": "ok", "excluded": []}],
            ["item.started", {"item_id": "i_c", "turn_id": "t_1", "started_at": 2, "kind": "tool_call",
                "tool_kind": "execute", "title": "Run", "paths": [], "command": "npm start", "cwd": "/w",
                "output_state": "none", "extraction": "full"}],
            ["item.completed", {"item_id": "i_c", "status": "cancelled", "ended_at": 5, "duration_ms": 1,
                "may_still_run": true}],
            ["snapshot.recorded", {"turn_id": "t_1", "phase": "end", "status": "partial",
                "excluded": too_large}],
            ["turn.completed", {"turn_id": "t_1", "status": "cancelled", "ended_at": 6, "duration_ms": 5,
                "final_item_ids": []}],
            ["turn.started", {"turn_id": "t_2", "turn_no": 2, "origin": "user", "started_at": 10}],
            ["item.started", {"item_id": "i_l", "turn_id": "t_2", "started_at": 11,
                "kind": "late_tool_output", "output_state": "streaming",
                "ref": {"item_id": "i_c", "turn_id": "t_1", "title": "Run", "command": "npm start"}}],
            ["item.completed", {"item_id": "i_l", "status": "completed", "ended_at": 20, "duration_ms": 9}],
            ["turn.completed", {"turn_id": "t_2", "status": "completed", "ended_at": 30, "duration_ms": 20,
                "final_item_ids": []}],
            ["turn.started", {"turn_id": "t_3", "turn_no": 3, "origin": "user", "started_at": 40}]
        ]);
        let mut view = SessionView::default();
        for event in events.as_array().unwrap() {
            let event = json!({"type": event[0], "data": event[1]});
            view.apply(&body(event)).unwrap();
        }
        let notes = |n: u32| turn_notes(&view, &format!("t_{n}").parse().unwrap());
        let (snapshot, late) = notes(1).unwrap();
        assert_eq!(snapshot.status, SnapshotStatus::Partial);
        assert_eq!(json!(snapshot.excluded), too_large);
        // The command ran on after turn 1 and through turn 2, not turn 3.
        let lates = [late, notes(2).unwrap().1, notes(3).unwrap().1];
        assert_eq!(lates, [true, true, false]);
        assert!(notes(4).is_none());
    }
}
