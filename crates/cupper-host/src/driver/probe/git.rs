//! git and git-lfs (design 5.2.10): their versions, checked at start against
//! the minimum git, and shown in the agents status. `/usr/bin/git` may be the
//! xcode-select stub, which opens an installation dialog when run without the
//! command-line tools; it is run only after `xcode-select -p` names a
//! developer directory that has `usr/bin/git`.

use std::path::{Path, PathBuf};

use cupper_sys::{Command, Env};

use super::run::run;
use super::version::{Version, probe};
use crate::clock::Clock;
use crate::durations::{Durations, VERSION_PROBE};

/// The oldest git the host supports (design 5.2.10, provisional).
pub const MIN_GIT: Version = Version {
    major: 2,
    minor: 39,
    patch: 0,
};

/// The xcode-select stub and `xcode-select` itself; tests use their own.
pub struct Stub {
    pub git: PathBuf,
    pub xcode_select: PathBuf,
}

impl Stub {
    pub fn system() -> Self {
        Self {
            git: "/usr/bin/git".into(),
            xcode_select: "/usr/bin/xcode-select".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Found(Version),
    Missing,
    /// `/usr/bin/git` is the stub: the command-line tools are not installed.
    NeedsCommandLineTools,
    /// It ran, but printed no version in time.
    Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitReport {
    pub git: Tool,
    pub git_lfs: Tool,
}

impl GitReport {
    /// The semantic codes for the agents status; empty when all is well.
    pub fn problems(&self) -> Vec<&'static str> {
        let git = match self.git {
            Tool::Found(version) if version < MIN_GIT => Some("git_too_old"),
            Tool::Found(_) => None,
            Tool::Missing => Some("git_missing"),
            Tool::NeedsCommandLineTools => Some("git_needs_command_line_tools"),
            Tool::Unreadable => Some("git_unreadable"),
        };
        let lfs = match self.git_lfs {
            Tool::Found(_) => None,
            Tool::Missing | Tool::NeedsCommandLineTools => Some("git_lfs_missing"),
            Tool::Unreadable => Some("git_lfs_unreadable"),
        };
        git.into_iter().chain(lfs).collect()
    }
}

/// The versions of `git` and `git_lfs`, the absolute paths CLI discovery
/// found (`None`: not found), each run with `env`.
pub async fn detect(
    clock: &dyn Clock,
    durations: &Durations,
    git: Option<&Path>,
    git_lfs: Option<&Path>,
    stub: &Stub,
    env: &Env,
) -> GitReport {
    let version = |path| async move {
        let found = probe(clock, durations, path, env.clone()).await;
        found.map_or(Tool::Unreadable, Tool::Found)
    };
    let git = match git {
        None => Tool::Missing,
        Some(path) if path == stub.git && !tools_installed(clock, durations, stub, env).await => {
            Tool::NeedsCommandLineTools
        }
        Some(path) => version(path).await,
    };
    let git_lfs = match git_lfs {
        None => Tool::Missing,
        Some(path) => version(path).await,
    };
    GitReport { git, git_lfs }
}

/// `xcode-select -p` names a developer directory that has `usr/bin/git`.
async fn tools_installed(clock: &dyn Clock, durations: &Durations, stub: &Stub, env: &Env) -> bool {
    let mut command = Command::new(&stub.xcode_select);
    command.arg("-p").env(env.clone());
    let Ok(output) = run(clock, command, durations.get(VERSION_PROBE)).await else {
        return false;
    };
    let dir = String::from_utf8_lossy(&output.stdout);
    let dir = Path::new(dir.trim());
    output.status.success() && dir.is_absolute() && dir.join("usr/bin/git").is_file()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::clock::ManualClock;
    use crate::driver::probe::testkit::script;
    use crate::store::testutil::TempDir;

    #[tokio::test]
    async fn unit_version_classify_git_minimum_and_missing_git_lfs() {
        let dir = TempDir::new("git-detect");
        let (clock, durations) = (ManualClock::new(), Durations::default());
        let old = script(dir.path(), "git-old", "echo 'git version 2.38.1'");
        let new = script(
            dir.path(),
            "git",
            "echo 'git version 2.39.5 (Apple Git-154)'",
        );
        let lfs = script(
            dir.path(),
            "git-lfs",
            "echo 'git-lfs/3.4.0 (GitHub; darwin arm64)'",
        );
        let stub = Stub::system();
        let env = Env::new();
        let report = detect(&clock, &durations, Some(&old), None, &stub, &env).await;
        assert_eq!(report.git, Tool::Found(Version::parse("2.38.1").unwrap()));
        assert_eq!(report.problems(), ["git_too_old", "git_lfs_missing"]);
        let report = detect(&clock, &durations, Some(&new), Some(&lfs), &stub, &env).await;
        assert_eq!(
            report.git_lfs,
            Tool::Found(Version::parse("3.4.0").unwrap())
        );
        assert!(report.problems().is_empty(), "{:?}", report.problems());
        let report = detect(&clock, &durations, None, None, &stub, &env).await;
        assert_eq!(report.problems(), ["git_missing", "git_lfs_missing"]);
    }

    #[tokio::test]
    async fn unit_version_classify_the_xcode_select_stub_is_never_run_without_the_tools() {
        let dir = TempDir::new("git-stub");
        let (clock, durations) = (ManualClock::new(), Durations::default());
        let ran = dir.path().join("stub-ran");
        let body = format!("touch {}\necho 'git version 2.39.5'", ran.display());
        let git = script(dir.path(), "git", &body);
        let developer = dir.path().join("Developer");
        let select = |name: &str, body: &str| script(dir.path(), name, body);
        let cases = [
            (
                select(
                    "no-tools",
                    "echo 'unable to get active developer directory' >&2; exit 2",
                ),
                false,
            ),
            (
                select("empty", &format!("echo {}", developer.display())),
                false,
            ),
            (
                select("tools", &format!("echo {}", developer.display())),
                true,
            ),
        ];
        for (xcode_select, installed) in cases {
            if installed {
                fs::create_dir_all(developer.join("usr/bin")).expect("mkdir");
                fs::write(developer.join("usr/bin/git"), "").expect("write");
            }
            let stub = Stub {
                git: git.clone(),
                xcode_select,
            };
            let report = detect(&clock, &durations, Some(&git), None, &stub, &Env::new()).await;
            let want = match installed {
                true => Tool::Found(Version::parse("2.39.5").unwrap()),
                false => Tool::NeedsCommandLineTools,
            };
            assert_eq!(report.git, want);
            assert_eq!(ran.exists(), installed, "the stub runs only with the tools");
        }
        let stub = Stub {
            git: git.clone(),
            xcode_select: dir.path().join("none"),
        };
        fs::remove_file(&ran).expect("reset the marker");
        let report = detect(&clock, &durations, Some(&git), None, &stub, &Env::new()).await;
        assert_eq!(
            report.problems(),
            ["git_needs_command_line_tools", "git_lfs_missing"]
        );
        assert!(!ran.exists(), "no xcode-select, no stub");
    }
}
