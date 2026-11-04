//! The LFS objects a repo tier write needs (design 5.2.10, T235). The
//! snapshot tier's git-lfs keeps the objects of what it cleans in cupper's
//! GIT_DIR; a pointer that a write puts in the user's index or work tree
//! needs its object in the user's store too, or a commit of it has none,
//! and a smudge goes to the LFS server for it.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, File, Permissions};
use std::io::{self, ErrorKind};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::RepoOp;
use super::rules::{Entry, truthy};
use super::snapshot::unique;
use super::stamp::Layout;

/// The oids of the pointers `op` writes with `patch`: those of its added
/// lines when staging, else those of its removed ones, as `-R` applies it.
pub fn wanted(patch: &[u8], op: RepoOp) -> BTreeSet<String> {
    let side: &[u8] = match op {
        RepoOp::Stage => b"+oid sha256:",
        _ => b"-oid sha256:",
    };
    let oids = patch
        .split(|b| *b == b'\n')
        .filter_map(|line| line.strip_prefix(side));
    let hex =
        |oid: &&[u8]| oid.len() == 64 && oid.iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    oids.filter(hex)
        .map(|oid| String::from_utf8_lossy(oid).into_owned())
        .collect()
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

/// Copies each of `oids` that `store` lacks from cupper's `private` store,
/// written aside in `<store>/tmp` and renamed into place once its sha256 is
/// its oid. One cupper does not have is left to the user's git-lfs.
pub fn supply(
    private: &Path,
    store: &Path,
    oids: &BTreeSet<String>,
    mode: Option<u32>,
) -> io::Result<()> {
    for oid in oids {
        let object = |root: &Path| {
            root.join("objects")
                .join(&oid[..2])
                .join(&oid[2..4])
                .join(oid)
        };
        let (from, to) = (object(private), object(store));
        if to.exists() || !from.exists() {
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
    Ok(())
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
