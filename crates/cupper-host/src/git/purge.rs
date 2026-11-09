//! The snapshots' storage (design 5.2.10, 5.4.5; T127): the bytes under a
//! directory, counted until a deadline, and the purge of every repository's
//! object store and LFS stores in `<data dir>/git/`.

use std::fs::{self, DirBuilder};
use std::io::{self, ErrorKind};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use tokio::sync::OwnedRwLockWriteGuard;

use super::Gates;
use super::snapshot::unique;
use crate::clock::{Clock, MonoTime};

/// Counts bytes until `clock` passes `deadline`; `partial` once it has.
pub struct Tally<'a> {
    pub clock: &'a dyn Clock,
    pub deadline: MonoTime,
    pub partial: bool,
}

impl Tally<'_> {
    /// The bytes of the files under `paths`, links not followed.
    pub fn bytes(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> u64 {
        let (mut todo, mut total): (Vec<PathBuf>, u64) = (paths.into_iter().collect(), 0);
        while let Some(path) = todo.pop() {
            if self.clock.now_mono() >= self.deadline {
                self.partial = true;
                break;
            }
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_dir() => todo.extend(entries(&path)),
                Ok(meta) => total += meta.len(),
                Err(_) => {}
            }
        }
        total
    }
}

/// Every repository's gate in `dir`, held alone.
pub struct Held {
    repos: Vec<PathBuf>,
    _gates: Vec<OwnedRwLockWriteGuard<()>>,
}

/// Takes the gates of the repositories in `dir` one by one, in the order of
/// their names. Only a purge holds more than one, so none waits on another.
pub async fn hold(dir: &Path, gates: Option<&Gates>) -> Held {
    let mut repos: Vec<PathBuf> = entries(dir)
        .filter(|r| r.join("objects").is_dir())
        .collect();
    repos.sort();
    let mut held = Vec::new();
    if let Some(gates) = gates {
        for repo in &repos {
            held.push(gates.hold(repo).await);
        }
    }
    Held {
        repos,
        _gates: held,
    }
}

impl Held {
    /// Renames each repository's object store and its work trees' LFS
    /// stores aside, makes an empty object store, and lets the gates go.
    /// What went aside, which no snapshot can reach.
    pub fn aside(self) -> io::Result<Vec<PathBuf>> {
        let (suffix, mut aside) = (format!(".purge-{}", unique()), Vec::new());
        for repo in &self.repos {
            let lfs = entries(&repo.join("ws")).map(|ws| ws.join("lfs"));
            for store in std::iter::once(repo.join("objects")).chain(lfs) {
                let mut to = store.clone().into_os_string();
                to.push(&suffix);
                match fs::rename(&store, &to) {
                    Err(err) if err.kind() == ErrorKind::NotFound => continue,
                    renamed => renamed?,
                }
                aside.push(to.into());
            }
            DirBuilder::new().mode(0o700).create(repo.join("objects"))?;
        }
        Ok(aside)
    }
}

/// Removes `path`, files and all, links not followed: the bytes of its files.
pub fn remove(path: &Path) -> io::Result<u64> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() {
        fs::remove_file(path)?;
        return Ok(meta.len());
    }
    let mut freed = 0;
    for entry in fs::read_dir(path)? {
        freed += remove(&entry?.path())?;
    }
    fs::remove_dir(path)?;
    Ok(freed)
}

/// Removes what purges of earlier runs left aside in `dir`, a crash before
/// they removed it; this run's own purges remove theirs.
pub fn leftovers(dir: &Path) {
    let ours = format!(".purge-{}.", std::process::id());
    for repo in entries(dir) {
        let lfs = entries(&repo.join("ws")).flat_map(|ws| entries(&ws));
        for path in entries(&repo).chain(lfs) {
            let name = path.file_name().unwrap_or_default().as_bytes();
            let aside = |mark: &[u8]| name.windows(mark.len()).any(|w| w == mark);
            if aside(b".purge-") && !aside(ours.as_bytes()) {
                let _ = remove(&path);
            }
        }
    }
}

fn entries(dir: &Path) -> impl Iterator<Item = PathBuf> + use<> {
    let read = fs::read_dir(dir).into_iter().flatten().flatten();
    read.map(|entry| entry.path())
}
