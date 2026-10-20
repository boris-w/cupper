//! `CliLocator` (design 5.6.6, 5.2.10 "程序的路径"): which `claude`, `git`
//! and `git-lfs` the host runs, and the environment an adapter gets.

pub mod resolve;
#[cfg(test)]
mod testkit;
pub mod trust;

use std::fs;
use std::path::{Path, PathBuf};

use cupper_protocol::index::CliInstall;

use resolve::{Layout, Resolved, is_executable, resolve};
use trust::Boundaries;

/// Where a CLI was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `agents.<agent>.cli_path` in the settings.
    Settings,
    /// A trusted entry of the login shell's `PATH`.
    LoginPath,
    /// A common install location of the [`Layout`], or nvm's default version.
    Common,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The native binary, as a realpath.
    pub binary: PathBuf,
    /// What was found, before resolving.
    pub found_at: PathBuf,
    pub source: Source,
    pub install: CliInstall,
}

/// No native binary; [`Missing::code`] is the semantic code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    NotFound,
    /// Installed, but without a native binary; the first such install.
    UnsupportedInstall {
        found_at: PathBuf,
    },
}

impl Missing {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "cli_not_found",
            Self::UnsupportedInstall { .. } => "unsupported_install",
        }
    }
}

/// Finds the programs the host runs, in trusted places only.
#[derive(Debug)]
pub struct CliLocator {
    boundaries: Boundaries,
    path: Vec<PathBuf>,
    home: PathBuf,
}

impl CliLocator {
    /// `login_path`: the login shell's `PATH`, or its fallback.
    pub fn new(boundaries: Boundaries, login_path: &str, home: PathBuf) -> Self {
        let path = boundaries.trusted_path(login_path);
        Self {
            boundaries,
            path,
            home,
        }
    }

    /// The trusted entries of the login shell's `PATH`, as realpaths: where
    /// `git` and `git-lfs` come from, and git's own `PATH` (design 5.2.10).
    pub fn trusted_path(&self) -> &[PathBuf] {
        &self.path
    }

    /// The CLI of `layout`. A `cli_path` from the settings is the only
    /// candidate when set; the user confirmed it natively (5.2.11), so the
    /// location rule does not apply to it. Otherwise the first candidate with
    /// a native binary wins: trusted `PATH` entries, then common locations.
    pub fn locate(&self, layout: &Layout, cli_path: Option<&Path>) -> Result<Found, Missing> {
        let candidates: Vec<(PathBuf, Source)> = match cli_path {
            Some(path) if path.is_absolute() => vec![(path.into(), Source::Settings)],
            Some(_) => Vec::new(),
            None => {
                let on_path = self
                    .path
                    .iter()
                    .map(|dir| (dir.join(layout.program), Source::LoginPath));
                on_path
                    .chain(self.common(layout).map(|p| (p, Source::Common)))
                    .collect()
            }
        };
        let mut unsupported = None;
        for (found_at, source) in candidates {
            let resolved = match source {
                Source::Settings => resolve(&found_at, layout, |p| fs::canonicalize(p).ok()),
                _ => resolve(&found_at, layout, |p| self.boundaries.trusted(p)),
            };
            match resolved {
                Some(Resolved::Native { binary, install }) => {
                    let found = Found {
                        binary,
                        found_at,
                        source,
                        install,
                    };
                    return Ok(found);
                }
                Some(Resolved::Unsupported) => {
                    unsupported.get_or_insert(found_at);
                }
                None => {}
            }
        }
        Err(match unsupported {
            Some(found_at) => Missing::UnsupportedInstall { found_at },
            None => Missing::NotFound,
        })
    }

    /// `git` or `git-lfs`: the first executable file of that name in a
    /// trusted `PATH` entry, as a realpath. Never a common location.
    pub fn tool(&self, name: &str) -> Option<PathBuf> {
        let in_path = self.path.iter().map(|dir| dir.join(name));
        in_path
            .filter_map(|path| self.boundaries.trusted(&path))
            .find(|path| is_executable(path))
    }

    fn common(&self, layout: &Layout) -> impl Iterator<Item = PathBuf> {
        let expand = |path: &&str| match path.strip_prefix("~/") {
            Some(rest) => self.home.join(rest),
            None => PathBuf::from(path),
        };
        let nvm = nvm_bin(&self.home).map(|bin| bin.join(layout.program));
        layout.common.iter().map(expand).chain(nvm)
    }
}

/// The bin directory of nvm's default version. `~/.nvm/alias/default` names
/// a version, a prefix of one or another alias (`lts/*` → `lts/jod` → ...);
/// the newest installed version that matches wins.
fn nvm_bin(home: &Path) -> Option<PathBuf> {
    let nvm = home.join(".nvm");
    let mut alias = "default".to_owned();
    for _ in 0..4 {
        match fs::read_to_string(nvm.join("alias").join(&alias)) {
            Ok(next) => alias = next.trim().to_owned(),
            Err(_) => break,
        }
    }
    let want = numbers(alias.trim_start_matches('v'))?;
    let versions = nvm.join("versions").join("node");
    let (_, newest) = fs::read_dir(&versions)
        .ok()?
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            let version = numbers(name.strip_prefix('v')?)?;
            version.starts_with(&want).then_some((version, name))
        })
        .max()?;
    Some(versions.join(newest).join("bin"))
}

/// `22.11.0` → `[22, 11, 0]`; nvm's `node` and `stable` match any version.
fn numbers(version: &str) -> Option<Vec<u64>> {
    if matches!(version, "node" | "stable") {
        return Some(Vec::new());
    }
    version.split('.').map(|part| part.parse().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::locate::resolve::CLAUDE;
    use crate::driver::locate::testkit::Tree;

    /// [`CLAUDE`] with its common locations below the sample home only.
    const SAMPLE: Layout = Layout {
        program: "claude",
        common: &["~/.local/bin/claude"],
        npm: CLAUDE.npm,
    };

    /// A locator whose `PATH` is `path` below `tree`, with the project at
    /// `home/project` and the temp dir at `tmp`.
    fn on_path(tree: &Tree, path: &[&str]) -> CliLocator {
        let path: Vec<String> = path
            .iter()
            .map(|p| tree.path(p).display().to_string())
            .collect();
        let boundaries = Boundaries::new([tree.path("home/project")], [tree.path("tmp")]);
        CliLocator::new(boundaries, &path.join(":"), tree.path("home"))
    }

    /// What `locate` found, relative to the tree.
    fn found(tree: &Tree, result: Result<Found, Missing>) -> (String, Source) {
        let found = result.expect("found");
        let binary = found.binary.strip_prefix(tree.root()).expect("in the tree");
        (binary.display().to_string(), found.source)
    }

    #[test]
    fn unit_cli_resolver_claude_lookup_order() {
        let tree = Tree::new(
            "locate-order",
            "
            home/project/bin/claude   macho
            js/bin/claude             exec #!/usr/bin/env node
            opt/bin/claude            macho
            home/.local/bin/claude    macho
            settings/claude           macho
            ",
        );
        let locator = on_path(&tree, &["home/project/bin", "js/bin", "opt/bin"]);
        // The project's copy is skipped, the JavaScript one passed over.
        let got = found(&tree, locator.locate(&SAMPLE, None));
        assert_eq!(got, ("opt/bin/claude".into(), Source::LoginPath));
        let settings = tree.path("settings/claude");
        let got = found(&tree, locator.locate(&SAMPLE, Some(&settings)));
        assert_eq!(got, ("settings/claude".into(), Source::Settings));
        // A settings path is the only candidate, even when it is unusable.
        let js = tree.path("js/bin/claude");
        let unsupported = Missing::UnsupportedInstall {
            found_at: js.clone(),
        };
        assert_eq!(locator.locate(&SAMPLE, Some(&js)), Err(unsupported));
        let relative = Path::new("settings/claude");
        assert_eq!(
            locator.locate(&SAMPLE, Some(relative)),
            Err(Missing::NotFound)
        );
        // Only JavaScript on PATH: the common location has the binary.
        let got = found(&tree, on_path(&tree, &["js/bin"]).locate(&SAMPLE, None));
        assert_eq!(got, ("home/.local/bin/claude".into(), Source::Common));
    }

    #[test]
    fn unit_cli_resolver_claude_nvm_default_and_not_found() {
        let tree = Tree::new(
            "locate-nvm",
            "
            home/.nvm/alias/default                      plain lts/*
            home/.nvm/alias/lts/*                        plain lts/jod
            home/.nvm/alias/lts/jod                      plain 22
            home/.nvm/versions/node/v22.9.0/bin/claude   macho
            home/.nvm/versions/node/v22.11.0/bin/claude  macho
            home/.nvm/versions/node/v24.1.0/bin/claude   macho
            ",
        );
        let got = found(&tree, on_path(&tree, &[]).locate(&SAMPLE, None));
        let newest_22 = "home/.nvm/versions/node/v22.11.0/bin/claude";
        assert_eq!(got, (newest_22.into(), Source::Common));
        let empty = Tree::new("locate-none", "home dir");
        let missing = on_path(&empty, &[]).locate(&SAMPLE, None).unwrap_err();
        assert_eq!(
            (missing.code(), missing),
            ("cli_not_found", Missing::NotFound)
        );
    }

    #[test]
    fn unit_realpath_git_and_git_lfs_come_from_trusted_path_entries() {
        let tree = Tree::new(
            "locate-tools",
            "
            home/project/bin/git     exec git
            usr/bin/git              exec git
            opt/bin/git-lfs          plain not executable
            usr/local/bin/git-lfs    exec git-lfs
            tmp/bin/git-lfs          exec git-lfs
            ",
        );
        let path = [
            "home/project/bin",
            "tmp/bin",
            "opt/bin",
            "usr/bin",
            "usr/local/bin",
        ];
        let locator = on_path(&tree, &path);
        let trusted = ["opt/bin", "usr/bin", "usr/local/bin"].map(|p| tree.path(p));
        assert_eq!(locator.trusted_path(), trusted);
        assert_eq!(locator.tool("git"), Some(tree.path("usr/bin/git")));
        assert_eq!(
            locator.tool("git-lfs"),
            Some(tree.path("usr/local/bin/git-lfs"))
        );
        assert_eq!(locator.tool("missing"), None);
    }
}
