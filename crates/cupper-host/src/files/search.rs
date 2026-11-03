//! `files/search` (design 5.2.11): a workspace's files, by `ls-files` in the
//! snapshot tier in a git work tree and by a walk that honours `.gitignore`
//! elsewhere, kept 5 s; the best fuzzy matches of a query, with the matched
//! parts in UTF-16 offsets for JS.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cupper_protocol::annotations::WirePath;
use cupper_protocol::methods::git::FileMatch;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ignore::WalkBuilder;

use crate::clock::{Clock, MonoTime};
use crate::durations::{Durations, FILE_LIST_TTL};
use crate::git::{GitTool, snapshot};

/// A list stops at this many files.
pub const MAX_FILES: usize = 200_000;
/// What a reply's paths and ranges take at most, well within a frame.
const REPLY_BYTES: usize = 48_000;

/// The file lists of workspaces, each for its time to live; a turn's end
/// drops its workspace's (design 5.2.11). Clones share them.
#[derive(Debug, Clone, Default)]
pub struct FileLists(Arc<Mutex<Lists>>);

/// Each workspace's files, and when they were listed.
type Lists = HashMap<PathBuf, (MonoTime, Arc<[String]>)>;

impl FileLists {
    /// A turn in `workspace` ended: its files may have changed.
    pub fn forget(&self, workspace: &Path) {
        self.lock().remove(workspace);
    }

    /// The files of `workspace`, listed again once its list is too old.
    pub async fn of(
        &self,
        workspace: &Path,
        git: Option<&GitTool>,
        clock: &dyn Clock,
        durations: &Durations,
    ) -> Arc<[String]> {
        let (now, ttl) = (clock.now_mono(), durations.get(FILE_LIST_TTL));
        let fresh = self
            .lock()
            .get(workspace)
            .filter(|(at, _)| now < *at + ttl)
            .cloned();
        if let Some((_, files)) = fresh {
            return files;
        }
        let files: Arc<[String]> = list(workspace, git, clock, durations).await.into();
        let listed = (now, Arc::clone(&files));
        self.lock().insert(workspace.to_owned(), listed);
        files
    }

    fn lock(&self) -> MutexGuard<'_, Lists> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A git work tree's files from the snapshot tier; a walk of any other
/// directory, or of one whose snapshot tier fails.
async fn list(
    workspace: &Path,
    git: Option<&GitTool>,
    clock: &dyn Clock,
    durations: &Durations,
) -> Vec<String> {
    if let Some(tool) = git {
        match snapshot::files(tool, workspace, clock, durations).await {
            Ok(listed) => {
                let paths = listed.split(|b| *b == 0).filter(|path| !path.is_empty());
                let paths = paths.filter_map(|path| std::str::from_utf8(path).ok());
                return paths.take(MAX_FILES).map(str::to_owned).collect();
            }
            Err(snapshot::ShotError::NotARepo(_)) => {}
            Err(err) => tracing::warn!("files/search walks the workspace: {err}"),
        }
    }
    let root = workspace.to_owned();
    tokio::task::spawn_blocking(move || walk(&root))
        .await
        .unwrap_or_default()
}

/// The files under `root`, hidden ones too, `.gitignore` honoured outside a
/// repository as well; no `.git` directory and no symbolic link followed.
fn walk(root: &Path) -> Vec<String> {
    let mut walk = WalkBuilder::new(root);
    walk.hidden(false).require_git(false);
    walk.filter_entry(|entry| entry.file_name() != ".git");
    let files = walk.build().flatten();
    let files = files.filter(|entry| entry.file_type().is_some_and(|t| t.is_file()));
    let paths = files.filter_map(|entry| {
        let path = entry.path().strip_prefix(root).ok()?;
        path.to_str().map(str::to_owned)
    });
    paths.take(MAX_FILES).collect()
}

/// The best `limit` matches of `query` in `files`: by score, then the
/// shorter path, then the path; no more than a reply holds.
pub fn best(files: &[String], query: &str, limit: usize) -> Vec<FileMatch> {
    let matcher = SkimMatcherV2::default();
    let scored = files
        .iter()
        .filter_map(|path| Some((matcher.fuzzy_match(path, query)?, path)));
    let mut scored: Vec<(i64, &String)> = scored.collect();
    scored.sort_by(|a, b| (b.0, a.1.len(), a.1).cmp(&(a.0, b.1.len(), b.1)));
    let mut room = REPLY_BYTES;
    let found = scored.into_iter().take(limit).map_while(|(score, path)| {
        let (_, picked) = matcher.fuzzy_indices(path, query)?;
        room = room.checked_sub(path.len() + 12 * picked.len() + 64)?;
        Some(FileMatch {
            path: WirePath(path.clone()),
            score: u32::try_from(score.max(0)).unwrap_or(u32::MAX),
            ranges_utf16: ranges(path, &picked),
        })
    });
    found.collect()
}

/// The characters `picked` of `path` (char indices, ascending) as
/// `[start, end)` runs in UTF-16 code units.
pub fn ranges(path: &str, picked: &[usize]) -> Vec<[u32; 2]> {
    let mut at = 0u32;
    let units: Vec<[u32; 2]> = (path.chars())
        .map(|c| {
            let start = at;
            at += u32::try_from(c.len_utf16()).unwrap_or(2);
            [start, at]
        })
        .collect();
    let mut runs: Vec<[u32; 2]> = Vec::new();
    for [start, end] in picked.iter().filter_map(|i| units.get(*i)).copied() {
        match runs.last_mut() {
            Some(run) if run[1] == start => run[1] = end,
            _ => runs.push([start, end]),
        }
    }
    runs
}
