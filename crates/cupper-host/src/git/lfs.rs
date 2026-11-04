//! The LFS objects a repo tier write needs (design 5.2.10, T235). The
//! snapshot tier's git-lfs keeps the objects of what it cleans in cupper's
//! GIT_DIR; a pointer that a write puts in the user's index or work tree
//! needs its object in the user's store too, or a commit of it has none,
//! and a smudge goes to the LFS server for it. A revert or a worktree add
//! that would smudge an object the user lacks is refused (T237).

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, File, Permissions};
use std::io::{self, ErrorKind};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use cupper_protocol::error::{ErrorCode, ErrorReason, RpcError};
use sha2::{Digest, Sha256};

use super::rules::{self, Entry, Origin, truthy};
use super::snapshot::{self, unique};
use super::stamp::Layout;
use super::{GitCmd, GitError, GitTool, RepoOp, RepoTier};
use crate::clock::Clock;
use crate::durations::Durations;

/// The oids of the pointers `op` writes with `patch`: those of its added
/// lines when staging, else those of its removed ones, as `-R` applies it.
pub fn wanted(patch: &[u8], op: RepoOp) -> BTreeSet<String> {
    match op {
        RepoOp::Stage => oids(patch, b"+oid sha256:"),
        _ => oids(patch, b"-oid sha256:"),
    }
}

/// The oids of the lines of `text` that are `prefix` and an oid.
fn oids(text: &[u8], prefix: &[u8]) -> BTreeSet<String> {
    let oids = text
        .split(|b| *b == b'\n')
        .filter_map(|line| line.strip_prefix(prefix));
    let hex =
        |oid: &&[u8]| oid.len() == 64 && oid.iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    oids.filter(hex)
        .map(|oid| String::from_utf8_lossy(oid).into_owned())
        .collect()
}

/// The oids of the LFS pointers a checkout of `tree` smudges, as git-lfs
/// finds pointers: blobs under 1024 bytes that are ones.
pub async fn pointers(
    tier: &RepoTier<'_>,
    tree: &OsStr,
    clock: &dyn Clock,
    durations: &Durations,
    writable: &[PathBuf],
) -> Result<BTreeSet<String>, GitError> {
    let list = GitCmd::repo(RepoOp::LsTree).args([tree]);
    let listed = tier
        .run(clock, durations, writable, list)
        .await?
        .output
        .stdout;
    // `<size> <oid>`; a submodule's size is `-`.
    let small = listed.split(|b| *b == 0).filter_map(|record| {
        let (size, oid) = record.split_at(record.iter().position(|b| *b == b' ')?);
        let size: usize = std::str::from_utf8(size).ok()?.parse().ok()?;
        (size < 1024).then(|| [&oid[1..], b"\n"].concat())
    });
    let small: Vec<u8> = small.flatten().collect();
    if small.is_empty() {
        return Ok(BTreeSet::new());
    }
    let read = GitCmd::repo(RepoOp::CatFile).input(small);
    let read = tier
        .run(clock, durations, writable, read)
        .await?
        .output
        .stdout;
    // `<oid> blob <size>\n<content>\n` each.
    let (mut out, mut found) = (&read[..], BTreeSet::new());
    while let Some(at) = out.iter().position(|b| *b == b'\n') {
        let size = out[..at].rsplit(|b| *b == b' ').next().unwrap_or_default();
        let Ok(size) = String::from_utf8_lossy(size).parse::<usize>() else {
            break;
        };
        let blob = out.get(at + 1..at + 1 + size).unwrap_or_default();
        if blob.starts_with(b"version https://git-lfs.github.com/spec/v1\n") {
            found.extend(oids(blob, b"oid sha256:"));
        }
        out = out.get(at + 2 + size..).unwrap_or_default();
    }
    Ok(found)
}

/// `git_error` with `lfs_object_missing` unless `missing` is empty.
pub fn none_missing(missing: &BTreeSet<String>) -> Result<(), RpcError> {
    let Some(first) = missing.first() else {
        return Ok(());
    };
    let more = missing.len() - 1;
    let message = format!("LFS object {first} and {more} more missing: `git lfs fetch` gets them");
    let error = RpcError::app(ErrorCode::GitError, message);
    Err(error.with_reason(ErrorReason::LfsObjectMissing))
}

/// Where a repository's LFS objects are, as its configuration has it.
#[derive(Debug, Clone)]
pub struct Stores {
    /// The user's store.
    user: PathBuf,
    /// cupper's, of the work tree's snapshots.
    private: PathBuf,
    /// The mode of what goes in the user's.
    mode: Option<u32>,
    /// The host runs git-lfs in the repository.
    pub runs: bool,
}

impl Stores {
    /// `work_tree`'s, its configuration read through the repo tier.
    pub async fn of(
        tool: &GitTool,
        work_tree: &Path,
        clock: &dyn Clock,
        durations: &Durations,
        writable: &[PathBuf],
    ) -> Result<Self, GitError> {
        let (tier, list) = (tool.repo_tier(work_tree), GitCmd::repo(RepoOp::ConfigList));
        let listed = tier.run(clock, durations, writable, list).await?;
        let mut entries = rules::parse(&listed.output.stdout, work_tree)?;
        // The repo tier's own `-c` overrides are not the repository's.
        entries.retain(|entry| entry.origin != Origin::CommandLine);
        let not_a_repo = |err| GitError::NotARepo(work_tree.to_owned(), err);
        let tree = work_tree.canonicalize().map_err(not_a_repo)?;
        let layout = Layout::read(&tree).map_err(not_a_repo)?;
        Ok(Self {
            user: store(&layout, &entries),
            private: snapshot::private(&tool.dir, &layout).1.join("lfs"),
            mode: shared(&entries),
            runs: snapshot::lfs_runs(&entries, tool.git_lfs()),
        })
    }
}

/// The user's LFS store as git-lfs finds it: `lfs.storage` of the git
/// configuration (`.lfsconfig` has no say), relative to the common dir,
/// else `<common dir>/lfs`.
pub fn store(layout: &Layout, entries: &[Entry]) -> PathBuf {
    let set = entries.iter().rev().find(|e| e.key == b"lfs.storage");
    let set = set
        .and_then(|e| e.value.as_deref())
        .filter(|v| !v.is_empty());
    layout
        .common_dir
        .join(OsStr::from_bytes(set.unwrap_or(b"lfs")))
}

/// The mode of the files git-lfs writes under `core.sharedRepository`, read
/// as git reads it; `None` leaves them to the umask.
pub fn shared(entries: &[Entry]) -> Option<u32> {
    let entry = entries
        .iter()
        .rev()
        .find(|e| e.key == b"core.sharedrepository")?;
    let Some(value) = entry.value.as_deref() else {
        return Some(0o660);
    };
    match (
        value,
        u32::from_str_radix(&String::from_utf8_lossy(value), 8),
    ) {
        (b"umask", _) | (_, Ok(0)) => None,
        (b"group", _) | (_, Ok(1)) => Some(0o660),
        (b"all" | b"world" | b"everybody", _) | (_, Ok(2)) => Some(0o664),
        (_, Ok(mode)) => (mode & 0o600 == 0o600).then_some(mode & 0o666),
        (_, Err(_)) => truthy(Some(value)).then_some(0o660),
    }
}

/// Copies each of `oids` that the user's store lacks from cupper's,
/// written aside in `<store>/tmp` and renamed into place once its
/// sha256 is its oid. The oids neither store has.
pub fn supply(stores: &Stores, oids: &BTreeSet<String>) -> io::Result<BTreeSet<String>> {
    let (store, mode, mut missing) = (&stores.user, stores.mode, BTreeSet::new());
    for oid in oids {
        let object = |root: &Path| {
            root.join("objects")
                .join(&oid[..2])
                .join(&oid[2..4])
                .join(oid)
        };
        let (from, to) = (object(&stores.private), object(store));
        if to.exists() {
            continue;
        }
        if !from.exists() {
            missing.insert(oid.clone());
            continue;
        }
        let tmp = store.join("tmp").join(format!("{oid}.{}", unique()));
        mkdir(tmp.parent().expect("tmp"), mode)?;
        mkdir(to.parent().expect("objects/xx/yy"), mode)?;
        let placed = copy(&from, &tmp, oid, mode).and_then(|()| fs::rename(&tmp, &to));
        if placed.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        placed?;
    }
    Ok(missing)
}

/// Copies `from` to `tmp`, on disk with `mode`, if what it copied is `oid`.
fn copy(from: &Path, tmp: &Path, oid: &str, mode: Option<u32>) -> io::Result<()> {
    fs::copy(from, tmp)?;
    let (mut file, mut sha) = (File::open(tmp)?, Sha256::new());
    io::copy(&mut file, &mut sha)?;
    if format!("{:x}", sha.finalize()) != oid {
        let err = format!("cupper's copy of LFS object {oid} is not its content");
        return Err(io::Error::new(ErrorKind::InvalidData, err));
    }
    mode.map_or(Ok(()), |mode| {
        file.set_permissions(Permissions::from_mode(mode))
    })?;
    file.sync_all()
}

/// Makes `dir` and the directories above it that are missing, as git-lfs
/// does: with `mode` and an `x` for each `r`, else as the umask has it.
fn mkdir(dir: &Path, mode: Option<u32>) -> io::Result<()> {
    match fs::create_dir(dir) {
        Err(err) if err.kind() == ErrorKind::AlreadyExists => return Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound && dir.parent().is_some() => {
            mkdir(dir.parent().expect("a parent"), mode)?;
            return mkdir(dir, mode);
        }
        made => made?,
    }
    let dir_mode = |mode: u32| Permissions::from_mode(mode | (mode & 0o444) >> 2);
    mode.map_or(Ok(()), |mode| fs::set_permissions(dir, dir_mode(mode)))
}
