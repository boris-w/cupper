//! Sample install trees for the locator's tests, built under a temp dir from
//! a table. The names in the tables are placeholders, never a real user's.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::gateway::testkit::TempDir;

pub enum Node {
    Dir,
    /// A symlink to this target, as written.
    Link(&'static str),
}

pub struct Tree {
    root: PathBuf,
    _dir: TempDir,
}

impl Tree {
    /// Creates each `(path, node)` in order below a fresh realpath'd root.
    pub fn new(name: &str, nodes: &[(&str, Node)]) -> Self {
        let dir = TempDir::new(name);
        let root = fs::canonicalize(dir.path()).expect("realpath of the temp dir");
        for (path, node) in nodes {
            let path = root.join(path);
            let parent = path.parent().expect("below the root");
            fs::create_dir_all(parent).expect("create parents");
            match node {
                Node::Dir => fs::create_dir_all(&path).expect("create a dir"),
                Node::Link(target) => symlink(target, &path).expect("create a symlink"),
            }
        }
        Self { root, _dir: dir }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}
