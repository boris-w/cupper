//! The discovery chain the host runs at start (design 5.6.6, 5.2.10): the
//! login shell's environment, then a [`CliLocator`] over its trusted `PATH`,
//! then the git the host runs. One [`Discovery`] serves the index's projects
//! (T034) and the agent runtimes (T055) alike.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use cupper_sys::Env;

use super::locate::CliLocator;
use super::locate::trust::{Boundaries, system_temp_dirs};
use super::probe::login_shell::{self, FALLBACK_PATH, Failure, LoginEnv};
use crate::clock::Clock;
use crate::durations::Durations;
use crate::git::GitTool;
use crate::manifest;

/// What the login shell starts with: the host's own variables of these names.
const SHELL_BASE: &[&str] = &[
    "HOME", "USER", "LOGNAME", "SHELL", "TMPDIR", "LANG", "LC_*", "PATH",
];

#[derive(Debug)]
pub struct Discovery {
    pub login_env: LoginEnv,
    pub locator: CliLocator,
    /// `None` when no trusted `PATH` entry has a git.
    pub git: Option<GitTool>,
}

/// Runs the chain. `roots`: every project and session workspace, where no
/// program may come from; the default worktree root joins them. `injected`:
/// `test_config.login_env`, which stands in for the login shell.
pub async fn discover(
    clock: &dyn Clock,
    durations: &Durations,
    data_dir: &Path,
    roots: Vec<PathBuf>,
    injected: Option<&BTreeMap<String, String>>,
) -> io::Result<Discovery> {
    let var = |name| std::env::var_os(name).filter(|value| !value.is_empty());
    let login_env = match (injected, var("SHELL")) {
        (Some(vars), _) => LoginEnv {
            vars: vars.clone(),
            failure: None,
        },
        (None, None) => LoginEnv::fallback(Failure::ShellMissing),
        (None, Some(shell)) => {
            let base = Env::from_allowed(std::env::vars_os(), SHELL_BASE);
            let wanted: Vec<&str> = manifest::embedded()
                .env
                .allow
                .iter()
                .map(String::as_str)
                .collect();
            login_shell::read(clock, durations, Path::new(&shell), base, &wanted).await
        }
    };
    let home = var("HOME").map(PathBuf::from);
    let tmpdir = var("TMPDIR").and_then(|dir| dir.into_string().ok());
    let worktrees = home.iter().map(|home| home.join(".cupper/worktrees"));
    let boundaries = Boundaries::new(
        roots.into_iter().chain(worktrees),
        system_temp_dirs(tmpdir.as_deref()),
    );
    let path = login_env
        .vars
        .get("PATH")
        .map_or(FALLBACK_PATH, String::as_str);
    let locator = CliLocator::new(boundaries, path, home.clone().unwrap_or_default());
    let git = match locator.tool("git") {
        Some(git) => {
            let ssh = login_env.vars.get("SSH_AUTH_SOCK").map(OsString::from);
            let lfs = locator.tool("git-lfs");
            Some(GitTool::new(
                git,
                lfs,
                locator.trusted_path(),
                home,
                ssh,
                data_dir,
            )?)
        }
        None => None,
    };
    Ok(Discovery {
        login_env,
        locator,
        git,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use crate::driver::probe::testkit::script;
    use crate::gateway::testkit::TempDir;

    /// `test_config.login_env` stands in for the shell; a `git` planted in a
    /// project is passed over for the system's.
    #[tokio::test]
    async fn discovery_takes_the_injected_environment_and_trusted_git_only() {
        let dir = TempDir::new("discover");
        let project = dir.path().join("project");
        std::fs::create_dir_all(project.join("bin")).unwrap();
        script(&project.join("bin"), "git", "echo planted");
        let path = format!("{}:/usr/bin:/bin", project.join("bin").display());
        let vars = BTreeMap::from([("PATH".to_owned(), path)]);
        let clock = ManualClock::new();
        let data_dir = dir.path().join("data");
        let durations = Durations::default();
        let found = discover(&clock, &durations, &data_dir, vec![project], Some(&vars));
        let found = found.await.unwrap();
        assert_eq!(found.login_env.failure, None);
        assert_eq!(
            found.locator.trusted_path(),
            [PathBuf::from("/usr/bin"), "/bin".into()]
        );
        assert_eq!(
            found.locator.tool("git"),
            Some(PathBuf::from("/usr/bin/git"))
        );
        assert!(found.git.is_some());
    }
}
