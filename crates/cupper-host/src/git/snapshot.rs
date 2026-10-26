//! Turn snapshots in the snapshot tier (design 5.2.10, "快照档"): the work
//! tree as a git tree, taken on cupper's own GIT_DIR, so that no program the
//! repository's configuration names runs and the user's `.git` is not
//! written.
//!
//! - A repository has a private object store `<data>/git/<repo>/objects/`
//!   over the repository's objects (alternates); a work tree a GIT_DIR
//!   `<data>/git/<repo>/ws/<work tree>/` with cupper's `config` ([`config`]),
//!   copies of `info/exclude`, `info/attributes`, `info/sparse-checkout` and
//!   the shared indexes, and a temporary index per snapshot. No hooks; every
//!   directory 0700.
//! - The index is the work tree's, copied with its mtime: a new one would
//!   hide a file changed within the index's second (racy git).
//! - Untracked files over 50 MB, nested repositories and wholly untracked
//!   directories of more than 10,000 files stay out. So do the files of a
//!   filter the configuration defines and the snapshot tier does not run:
//!   they get a placeholder blob, and no plaintext reaches the store.
//! - Then `add -A` and `write-tree`; at a turn's end, the lines changed
//!   since its start.

mod config;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs::{self, DirBuilder, File, FileTimes};
use std::io::{self, ErrorKind};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use cupper_protocol::annotations::WirePath;
use cupper_protocol::session::{DiffStat, ExcludedPath, ExclusionReason};
use sha2::{Digest, Sha256};

use super::rules::{self, Unsafe};
use super::run::RunError;
use super::stamp::Layout;
use super::{GitCmd, GitTool, RepoOp, Snapshot, SnapshotOp, SnapshotTier};
use crate::clock::Clock;
use crate::durations::Durations;

/// Untracked files over this stay out.
pub const TOO_LARGE: u64 = 50 << 20;
/// Wholly untracked directories of more files than this stay out.
pub const LARGE_DIR: usize = 10_000;

/// Where the snapshot tier runs for a work tree.
#[derive(Debug, Clone)]
pub struct Place {
    pub work_tree: PathBuf,
    pub git_dir: PathBuf,
    pub index: PathBuf,
    pub objects: PathBuf,
    /// The repository's object directory and its alternates, `:`-separated.
    pub alternates: OsString,
}

/// A snapshot: its tree id, kept in the host, and what it left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken {
    pub tree: String,
    pub excluded: Vec<ExcludedPath>,
    /// `add` could not read every file.
    pub partial: bool,
    /// Against the start tree, when there was one.
    pub stat: Option<DiffStat>,
}

#[derive(Debug, thiserror::Error)]
pub enum ShotError {
    #[error("{0} is not a git work tree")]
    NotARepo(PathBuf),
    #[error("cannot prepare the snapshot")]
    Io(#[from] io::Error),
    #[error("the configuration cannot be read")]
    Config(#[from] Unsafe),
    #[error("cannot run git")]
    Run(#[from] RunError),
    #[error("git {0} failed: {1}")]
    Failed(&'static str, String),
}

/// Takes a snapshot of `work_tree`; with `start`, the lines changed since
/// that tree.
pub async fn take(
    tool: &GitTool,
    work_tree: &Path,
    start: Option<&str>,
    clock: &dyn Clock,
    durations: &Durations,
) -> Result<Taken, ShotError> {
    let tree = work_tree.to_owned();
    let layout = blocking(move || {
        let not_a_repo = |_| ShotError::NotARepo(tree.clone());
        Layout::read(&tree.canonicalize().map_err(not_a_repo)?).map_err(not_a_repo)
    });
    let layout = layout.await?;
    // Reading the configuration runs nothing (design 5.2.10).
    let listing = tool.repo_tier(&layout.work_tree);
    let listing = listing.prepare(GitCmd::repo(RepoOp::ConfigList));
    let listing = listing.run(clock, durations).await?;
    if !listing.status.success() {
        return Err(failed("config", &listing.stderr));
    }
    let entries = rules::parse(&listing.stdout, &layout.work_tree)?;
    let (text, unrun) = (
        config::text(&entries, tool.git_lfs()),
        config::unrun(&entries, tool.git_lfs()),
    );
    let dir = tool.dir.clone();
    let place = blocking(move || prepare(&dir, &layout, &text)).await?;
    let taken = shoot(&tool.snapshot_tier(&place), &unrun, start, clock, durations).await;
    let index = place.index.clone();
    // A leftover temporary index costs space only.
    let _ = blocking(move || Ok(fs::remove_file(index)?)).await;
    taken
}

/// The GIT_DIR of `layout`'s work tree, with a fresh copy of its index.
fn prepare(dir: &Path, layout: &Layout, config: &[u8]) -> Result<Place, ShotError> {
    let key = |path: &Path| {
        let hash = Sha256::digest(path.as_os_str().as_bytes());
        hash[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let repo = dir.join(key(&layout.common_dir));
    let git_dir = repo.join("ws").join(key(&layout.work_tree));
    let mut mkdir = DirBuilder::new();
    mkdir.recursive(true).mode(0o700);
    for sub in [
        repo.join("objects"),
        git_dir.join("info"),
        git_dir.join("refs"),
    ] {
        mkdir.create(sub)?;
    }
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/snapshot\n")?;
    let unique = unique();
    replace(&git_dir.join("config"), &unique, |tmp| {
        fs::write(tmp, config)
    })?;
    let common = &layout.common_dir;
    for (from, name) in [
        (common.join("info/exclude"), "info/exclude"),
        (common.join("info/attributes"), "info/attributes"),
        (
            layout.git_dir.join("info/sparse-checkout"),
            "info/sparse-checkout",
        ),
    ] {
        let to = git_dir.join(name);
        replace(&to, &unique, |tmp| copy(&from, tmp))?;
    }
    for entry in fs::read_dir(&layout.git_dir)? {
        let name = entry?.file_name();
        if name.as_bytes().starts_with(b"sharedindex.") {
            copy(&layout.git_dir.join(&name), &git_dir.join(&name))?;
        }
    }
    // A repository without commits may have no index yet.
    let index = git_dir.join(format!("index.{unique}"));
    match copy(&layout.git_dir.join("index"), &index) {
        Err(err) if err.kind() != ErrorKind::NotFound => return Err(err.into()),
        _ => {}
    }
    let objects = common.join("objects");
    let mut alternates = vec![objects.clone()];
    match fs::read(objects.join("info/alternates")) {
        Ok(lines) => {
            let lines = lines.split(|b| *b == b'\n').map(<[u8]>::trim_ascii);
            let lines = lines.filter(|line| !line.is_empty() && !line.starts_with(b"#"));
            alternates.extend(lines.map(|line| objects.join(OsStr::from_bytes(line))));
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }
    let alternates = std::env::join_paths(alternates).map_err(io::Error::other)?;
    let (work_tree, objects) = (layout.work_tree.clone(), repo.join("objects"));
    Ok(Place {
        work_tree,
        git_dir,
        index,
        objects,
        alternates,
    })
}

/// A name no other snapshot of this host uses at the same time.
fn unique() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}.{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Writes `to` through a temporary file, which `write` fills, or removes it
/// when `write` finds nothing to write.
fn replace(to: &Path, unique: &str, write: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
    let mut tmp = to.as_os_str().to_owned();
    tmp.push(format!(".{unique}"));
    match write(Path::new(&tmp)) {
        Ok(()) => fs::rename(&tmp, to),
        Err(err) if err.kind() == ErrorKind::NotFound => match fs::remove_file(to) {
            Err(err) if err.kind() != ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        },
        Err(err) => Err(err),
    }
}

/// Copies `from` with its mtime, as `cp -p` does; `NotFound` without it.
fn copy(from: &Path, to: &Path) -> io::Result<()> {
    let modified = fs::metadata(from)?.modified()?;
    fs::copy(from, to)?;
    let file = File::options().write(true).open(to)?;
    file.set_times(FileTimes::new().set_modified(modified))
}

/// Steps 2–4 of a snapshot, on its GIT_DIR.
async fn shoot(
    tier: &SnapshotTier<'_>,
    unrun: &BTreeSet<Vec<u8>>,
    start: Option<&str>,
    clock: &dyn Clock,
    durations: &Durations,
) -> Result<Taken, ShotError> {
    let call = |cmd: GitCmd<Snapshot>, what| async move {
        let output = tier.prepare(cmd).run(clock, durations).await?;
        match output.status.success() {
            true => Ok(output.stdout),
            false => Err(failed(what, &output.stderr)),
        }
    };
    let work_tree = tier.place.work_tree.clone();
    let untracked = call(GitCmd::snapshot(SnapshotOp::Untracked), "ls-files").await?;
    let dirs = call(GitCmd::snapshot(SnapshotOp::UntrackedDirs), "ls-files").await?;
    let tree = work_tree.clone();
    let mut out = blocking(move || Ok(precheck(&tree, &untracked, &dirs)?)).await?;
    let mut entries = Vec::new();
    if !unrun.is_empty() {
        let paths = call(GitCmd::snapshot(SnapshotOp::LsFiles), "ls-files").await?;
        let check = GitCmd::snapshot(SnapshotOp::CheckAttr).input(paths);
        let attrs = call(check, "check-attr").await?;
        let filtered = filtered(&attrs, unrun);
        let tree = work_tree.clone();
        let placeholders = blocking(move || Ok(placeholders(&tree, filtered)?)).await?;
        for (path, mode, text) in placeholders {
            let hash = GitCmd::snapshot(SnapshotOp::HashObject).input(text);
            let blob = call(hash, "hash-object").await?;
            let blob = String::from_utf8_lossy(blob.trim_ascii()).into_owned();
            entries.extend(
                format!("{mode:o} {blob}\t")
                    .bytes()
                    .chain(path.iter().copied()),
            );
            entries.push(0);
            out.push((path, ExclusionReason::FilterUnsupported));
        }
    }
    let mut pathspecs = b":/\0".to_vec();
    for (path, _) in &out {
        pathspecs.extend(b":(exclude,top,literal)");
        pathspecs.extend(path);
        pathspecs.push(0);
    }
    let add = (GitCmd::snapshot(SnapshotOp::AddAll))
        .config("core.safecrlf", "false")
        .config("advice.addEmbeddedRepo", "false")
        .input(pathspecs);
    let partial = !tier
        .prepare(add)
        .run(clock, durations)
        .await?
        .status
        .success();
    if !entries.is_empty() {
        let info = GitCmd::snapshot(SnapshotOp::IndexInfo).input(entries);
        call(info, "update-index").await?;
    }
    let tree = call(GitCmd::snapshot(SnapshotOp::WriteTree), "write-tree").await?;
    let tree = String::from_utf8_lossy(tree.trim_ascii()).into_owned();
    let stat = match start {
        Some(start) => {
            let diff = GitCmd::snapshot(SnapshotOp::DiffStat).args([start, tree.as_str()]);
            Some(stat(&call(diff, "diff-tree").await?))
        }
        None => None,
    };
    let excluded = out.into_iter().map(|(path, reason)| {
        let path = WirePath(String::from_utf8_lossy(&path).into_owned());
        ExcludedPath { path, reason }
    });
    Ok(Taken {
        tree,
        excluded: excluded.collect(),
        partial,
        stat,
    })
}

/// What stays out of `untracked` (`ls-files -o`, nested repositories as
/// `dir/`) given `dirs` (the same with wholly untracked directories as
/// `dir/`).
fn precheck(work_tree: &Path, untracked: &[u8], dirs: &[u8]) -> io::Result<Excluded> {
    let files: Vec<&[u8]> = split(untracked).collect();
    let within = |dir: &&[u8]| files.iter().filter(|f| f.starts_with(dir)).count() > LARGE_DIR;
    let large: Vec<&[u8]> = split(dirs)
        .filter(|d| d.ends_with(b"/"))
        .filter(within)
        .collect();
    let mut out: Excluded = (large.iter())
        .map(|dir| {
            (
                dir[..dir.len() - 1].to_vec(),
                ExclusionReason::LargeUntrackedDir,
            )
        })
        .collect();
    for file in files {
        if large.iter().any(|dir| file.starts_with(dir)) {
            continue;
        }
        let size = || {
            Ok::<_, io::Error>(fs::symlink_metadata(work_tree.join(OsStr::from_bytes(file)))?.len())
        };
        if let Some(repo) = file.strip_suffix(b"/") {
            out.push((repo.to_vec(), ExclusionReason::NestedRepo));
        } else if size()? > TOO_LARGE {
            out.push((file.to_vec(), ExclusionReason::TooLarge));
        }
    }
    Ok(out)
}

/// A placeholder's path, mode and content.
type Placeholder = (Vec<u8>, u32, Vec<u8>);

/// Paths left out, as git wrote them, and why.
type Excluded = Vec<(Vec<u8>, ExclusionReason)>;

/// The paths of `check-attr -z` output whose filter is one of `unrun`.
fn filtered(output: &[u8], unrun: &BTreeSet<Vec<u8>>) -> Vec<(Vec<u8>, Vec<u8>)> {
    let fields: Vec<&[u8]> = split(output).collect();
    let triples = fields.chunks_exact(3);
    let hits = triples.filter(|t| unrun.contains(t[2]));
    hits.map(|t| (t[0].to_vec(), t[2].to_vec())).collect()
}

/// The placeholder of each filtered file the work tree has: its path, mode
/// and content, which names the filter and the hash of the file.
fn placeholders(
    work_tree: &Path,
    filtered: Vec<(Vec<u8>, Vec<u8>)>,
) -> io::Result<Vec<Placeholder>> {
    let mut out = Vec::new();
    for (path, filter) in filtered {
        let file = work_tree.join(OsStr::from_bytes(&path));
        let meta = match fs::symlink_metadata(&file) {
            Ok(meta) if meta.is_file() => meta,
            // Gone or not a file: `add -A` records what it is.
            _ => continue,
        };
        let hash = Sha256::digest(fs::read(&file)?);
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        let filter = String::from_utf8_lossy(&filter);
        let text = format!("cupper: not stored, filter {filter} not run; sha-256 {hex}\n");
        let mode = match meta.permissions().mode() & 0o111 {
            0 => 0o100_644,
            _ => 0o100_755,
        };
        out.push((path, mode, text.into_bytes()));
    }
    Ok(out)
}

/// `diff-tree -z --numstat` output summed up; a binary file counts no lines.
fn stat(output: &[u8]) -> DiffStat {
    let counts = |n: &[u8]| {
        std::str::from_utf8(n)
            .ok()
            .and_then(|n| n.parse::<u32>().ok())
    };
    let mut stat = DiffStat {
        files: 0,
        added: 0,
        removed: 0,
    };
    for line in split(output) {
        let mut fields = line.splitn(3, |b| *b == b'\t');
        let (added, removed) = (
            fields.next().and_then(counts),
            fields.next().and_then(counts),
        );
        stat.files += 1;
        stat.added += added.unwrap_or(0);
        stat.removed += removed.unwrap_or(0);
    }
    stat
}

fn split(output: &[u8]) -> impl Iterator<Item = &[u8]> {
    output.split(|b| *b == 0).filter(|field| !field.is_empty())
}

fn failed(what: &'static str, stderr: &[u8]) -> ShotError {
    let tail = String::from_utf8_lossy(&stderr[stderr.len().saturating_sub(512)..]);
    ShotError::Failed(what, tail.trim_end().to_owned())
}

/// File system work off the async runtime (T109, T138).
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, ShotError> + Send + 'static,
) -> Result<T, ShotError> {
    let joined = tokio::task::spawn_blocking(work).await;
    joined.map_err(|err| ShotError::Io(io::Error::other(err)))?
}
