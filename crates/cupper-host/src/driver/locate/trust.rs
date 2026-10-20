//! Where a program the host runs may live (design 5.2.10 "程序的路径", 5.6.6).
//!
//! Agents can write to projects, workspaces and temp dirs, so a `claude` or
//! `git` found there may be one an agent planted. A `PATH` entry is trusted
//! when it is absolute and its realpath lies outside all of them; comparing
//! realpaths catches an entry that reaches in through a symlink.

use std::fs;
use std::path::{Path, PathBuf};

/// The temp dirs besides `$TMPDIR`.
pub const SYSTEM_TEMP_DIRS: &[&str] = &["/tmp", "/private/var/folders"];

/// `$TMPDIR`, when set, and [`SYSTEM_TEMP_DIRS`].
pub fn system_temp_dirs(tmpdir: Option<&str>) -> Vec<PathBuf> {
    tmpdir
        .into_iter()
        .chain(SYSTEM_TEMP_DIRS.iter().copied())
        .map(PathBuf::from)
        .collect()
}

/// The directories agents can write to, as realpaths.
#[derive(Debug, Clone)]
pub struct Boundaries {
    roots: Vec<PathBuf>,
    temp: Vec<PathBuf>,
}

impl Boundaries {
    /// `roots`: every project and workspace root and the worktree root, from
    /// the caller; `temp`: the temp dirs, in production [`system_temp_dirs`].
    pub fn new(
        roots: impl IntoIterator<Item = PathBuf>,
        temp: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        let real = |path: PathBuf| fs::canonicalize(&path).unwrap_or(path);
        Self {
            roots: roots.into_iter().map(real).collect(),
            temp: temp.into_iter().map(real).collect(),
        }
    }

    /// The realpath of `path` if `path` is absolute, exists, and its
    /// realpath lies outside every root and temp dir.
    pub fn trusted(&self, path: &Path) -> Option<PathBuf> {
        if !path.is_absolute() {
            return None;
        }
        let real = fs::canonicalize(path).ok()?;
        (self.outside_roots(&real) && !within(&real, &self.temp)).then_some(real)
    }

    /// Whether the realpath `real` lies outside every project, workspace and
    /// the worktree root. Temp dirs do not count here (config dirs, 5.6.6).
    pub fn outside_roots(&self, real: &Path) -> bool {
        !within(real, &self.roots)
    }

    /// The trusted entries of a `PATH` value: realpaths, in order, each once.
    pub fn trusted_path(&self, path_var: &str) -> Vec<PathBuf> {
        let mut entries: Vec<PathBuf> = Vec::new();
        for entry in path_var.split(':') {
            if let Some(real) = self.trusted(Path::new(entry))
                && !entries.contains(&real)
            {
                entries.push(real);
            }
        }
        entries
    }
}

/// Whether `path` is one of `dirs` or below one, component by component.
fn within(path: &Path, dirs: &[PathBuf]) -> bool {
    dirs.iter().any(|dir| path.starts_with(dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::locate::testkit::Tree;

    #[test]
    fn unit_realpath_trusted_path_skips_entries_agents_can_write() {
        // `bin` and `opt` look outside but are not, after realpath.
        let tree = Tree::new(
            "trust",
            "
            usr/bin                                       dir
            home/user/projects/app/bin                    dir
            home/user/projects/apple/bin                  dir
            home/user/workspaces/s1/bin                   dir
            home/user/.cupper/worktrees/app-1a2b/s2/bin   dir
            tmp/bin                                       dir
            home/user/bin                                 -> projects/app/bin
            home/user/opt                                 -> ../../usr
            ",
        );
        let boundaries = Boundaries::new(
            ["projects/app", "workspaces/s1", ".cupper/worktrees"]
                .map(|root| tree.path(&format!("home/user/{root}"))),
            [tree.path("tmp")],
        );
        let entries = "usr/bin home/user/projects/app/bin home/user/workspaces/s1/bin
            home/user/.cupper/worktrees/app-1a2b/s2/bin home/user/.cupper/worktrees tmp/bin
            home/user/bin home/user/projects/apple/bin home/user/opt/bin home/user/missing";
        let entries = entries
            .split_whitespace()
            .map(|e| tree.path(e).display().to_string());
        let mut path_var: Vec<String> = entries.collect();
        path_var.extend(["usr/bin", "", "."].map(String::from));
        // A component prefix is not containment: `apple` is not inside `app`.
        // `opt/bin` is `usr/bin` again after realpath, so it appears once.
        assert_eq!(
            boundaries.trusted_path(&path_var.join(":")),
            [
                tree.path("usr/bin"),
                tree.path("home/user/projects/apple/bin")
            ]
        );
    }

    #[test]
    fn unit_realpath_temp_dirs_are_out_but_config_dirs_may_use_them() {
        let boundaries = Boundaries::new([], system_temp_dirs(Some("/private/var/folders")));
        assert_eq!(
            boundaries.trusted(Path::new("/usr/bin")),
            Some("/usr/bin".into())
        );
        // `/tmp` and `/var/folders` are symlinks into `/private`.
        for temp in ["/tmp", "/var/folders", "/private/var/folders"] {
            assert_eq!(boundaries.trusted(Path::new(temp)), None, "{temp}");
        }
        assert!(boundaries.outside_roots(Path::new("/private/tmp")));
        let tree = Tree::new("trust-roots", "project/conf dir");
        let boundaries = Boundaries::new([tree.path("project")], []);
        assert!(!boundaries.outside_roots(&tree.path("project/conf")));
        assert!(boundaries.outside_roots(tree.root()));
    }
}
