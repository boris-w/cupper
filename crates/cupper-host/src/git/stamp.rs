//! The fingerprint of a repository's executable configuration (design
//! 5.2.10, "仓库可执行配置指纹"): every source file of the effective
//! configuration (the repository's `config` and `config.worktree` even
//! while missing), the hooks in `hooks/` and in `core.hooksPath`,
//! `info/attributes`, and every `.gitattributes` of the work tree. Taken
//! before and after a repo tier call and at a turn's start and end; read
//! only, and git does not run.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::io::{self, ErrorKind};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::rules::{Entry, Origin};

/// Where a work tree keeps its git data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub work_tree: PathBuf,
    /// The work tree's own: `.git`, or `<common>/worktrees/<name>`.
    pub git_dir: PathBuf,
    /// Shared by the repository's work trees: config, hooks, info.
    pub common_dir: PathBuf,
}

impl Layout {
    /// The layout of `work_tree`: `.git` is the git dir, or a file
    /// `gitdir: <path>` (a linked worktree) whose git dir names the common
    /// dir in `commondir`. Paths in both are relative to where they are; the
    /// git dir and the common dir come back as realpaths.
    pub fn read(work_tree: &Path) -> io::Result<Self> {
        let dot = work_tree.join(".git");
        let git_dir = if dot.is_dir() {
            dot
        } else {
            let text = fs::read(&dot)?;
            let path = text
                .strip_prefix(b"gitdir: ")
                .ok_or(ErrorKind::InvalidData)?;
            work_tree.join(OsStr::from_bytes(path.trim_ascii_end()))
        };
        let git_dir = git_dir.canonicalize()?;
        let common_dir = match fs::read(git_dir.join("commondir")) {
            Ok(text) => git_dir.join(OsStr::from_bytes(text.trim_ascii_end())),
            Err(err) if err.kind() == ErrorKind::NotFound => git_dir.clone(),
            Err(err) => return Err(err),
        };
        let common_dir = common_dir.canonicalize()?;
        let work_tree = work_tree.to_owned();
        Ok(Self {
            work_tree,
            git_dir,
            common_dir,
        })
    }
}

/// Each file covered, with the sha-256 of its content, `None` while missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(BTreeMap<PathBuf, Option<[u8; 32]>>);

impl Fingerprint {
    /// The fingerprint of `layout` now, with the sources of `config`, the
    /// effective configuration read for the call.
    pub fn take(layout: &Layout, config: &[Entry]) -> io::Result<Self> {
        let mut files = BTreeMap::new();
        let mut add = |path: PathBuf| -> io::Result<()> {
            let content = match fs::read(&path) {
                Ok(bytes) => Some(Sha256::digest(bytes).into()),
                Err(err) if err.kind() == ErrorKind::NotFound => None,
                Err(err) => return Err(err),
            };
            files.insert(path, content);
            Ok(())
        };
        let sources = config.iter().filter_map(|entry| match &entry.origin {
            Origin::File(path) => Some(path.clone()),
            _ => None,
        });
        let common = &layout.common_dir;
        let own = [
            common.join("config"),
            layout.git_dir.join("config.worktree"),
        ];
        let attributes = common.join("info").join("attributes");
        for path in sources.chain(own).chain([attributes]) {
            add(path)?;
        }
        // The last `core.hooksPath` is in effect; a relative one is from the
        // work tree's root.
        let hooks_path = config.iter().rev().find(|e| e.key == b"core.hookspath");
        let hooks_path = hooks_path.and_then(|e| e.value.as_deref());
        let hooks_path = hooks_path.map(|path| layout.work_tree.join(OsStr::from_bytes(path)));
        for dir in [Some(common.join("hooks")), hooks_path]
            .into_iter()
            .flatten()
        {
            for path in listing(&dir)? {
                if !path.is_dir() {
                    add(path)?;
                }
            }
        }
        attributes_files(&layout.work_tree, &mut add)?;
        Ok(Self(files))
    }

    /// The paths that differ in `after`: changed, appeared or gone.
    pub fn changed(&self, after: &Self) -> Vec<PathBuf> {
        let paths: BTreeSet<&PathBuf> = self.0.keys().chain(after.0.keys()).collect();
        let differ = |path: &&PathBuf| self.0.get(*path) != after.0.get(*path);
        paths.into_iter().filter(differ).cloned().collect()
    }
}

/// The entries of `dir`; none while it is missing.
fn listing(dir: &Path) -> io::Result<Vec<PathBuf>> {
    match fs::read_dir(dir) {
        Ok(entries) => entries.map(|entry| Ok(entry?.path())).collect(),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

/// Every `.gitattributes` under `dir`, skipping `.git` and not following
/// symbolic links to directories.
fn attributes_files(dir: &Path, add: &mut impl FnMut(PathBuf) -> io::Result<()>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        if entry.file_type()?.is_dir() {
            attributes_files(&entry.path(), add)?;
        } else if name == ".gitattributes" {
            add(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
