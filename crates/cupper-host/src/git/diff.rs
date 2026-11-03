//! `git/diff` and `git/file_diff` in the snapshot tier (design 5.2.10, "diff
//! 与 hunk"). Every range compares two trees: a turn's start and end
//! snapshots, or its start and a snapshot taken now while it runs; the
//! index and a snapshot taken now (`unstaged`); HEAD and the index
//! (`staged`). Trees never change, so a file's patch is the same whenever it
//! is made: the cache keeps the trees and the file list, and
//! `git/file_diff` makes one file's patch when asked.

mod cache;
mod hunks;

use std::collections::BTreeSet;

use cupper_protocol::annotations::WirePath;
use cupper_protocol::ids::TurnId;
use cupper_protocol::item::{DiffChange, Item};
use cupper_protocol::methods::git::{DiffSnapshot, GitDiffFile};
use cupper_protocol::session::{DiffStat, SnapshotStatus};
use cupper_protocol::view::{Entry, ItemView, SessionView};

pub use cache::DiffCache;
pub use hunks::units;

use super::snapshot::{self, Opened, Place, ShotError, Taken, output, shoot};
use super::stamp::Fingerprint;
use super::{GitCmd, GitTool, SnapshotOp};
use crate::clock::Clock;
use crate::durations::Durations;

/// What a diff compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Range {
    Unstaged,
    /// HEAD's tree; `None` before the first commit.
    Staged(Option<String>),
    /// A turn's start tree and end tree, `None` while it runs.
    Turn(String, Option<String>),
}

/// A diff made.
#[derive(Debug, Clone)]
pub struct Diffed {
    /// Where its patches are made.
    pub place: Place,
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
    work_tree: &std::path::Path,
    range: Range,
    clock: &dyn Clock,
    durations: &Durations,
) -> Result<Diffed, ShotError> {
    let opened = snapshot::open(tool, work_tree, clock, durations).await?;
    let made = make(tool, &opened, range, clock, durations).await;
    let place = opened.place.clone();
    opened.close().await;
    let (old, new, files, taken) = made?;
    Ok(Diffed {
        place,
        old,
        new,
        files,
        taken,
    })
}

type Made = (String, String, Vec<GitDiffFile>, Option<Taken>);

async fn make(
    tool: &GitTool,
    opened: &Opened,
    range: Range,
    clock: &dyn Clock,
    durations: &Durations,
) -> Result<Made, ShotError> {
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
    Ok((old, new, files, taken))
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

/// The patch of `file` between `old` and `new`: the file alone, not what is
/// under a directory of its name; with a rename's source.
pub async fn patch(
    tool: &GitTool,
    diffed: &Diffed,
    file: &GitDiffFile,
    clock: &dyn Clock,
    durations: &Durations,
) -> Result<Vec<u8>, ShotError> {
    let mut args = vec![diffed.old.clone(), diffed.new.clone(), "--".into()];
    for path in std::iter::once(&file.path).chain(&file.old_path) {
        args.push(format!(":(literal){}", path.0));
        args.push(format!(":(exclude,literal){}/", path.0));
    }
    let cmd = GitCmd::snapshot(SnapshotOp::DiffPatch).args(args);
    let tier = tool.snapshot_tier(&diffed.place);
    output(&tier, cmd, "diff-tree", clock, durations).await
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
        if shot.status != SnapshotStatus::Ok {
            snapshot.status = shot.status.clone();
        }
        let new = shot
            .excluded
            .iter()
            .filter(|e| !snapshot.excluded.contains(e));
        snapshot.excluded.extend(new.cloned().collect::<Vec<_>>());
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
