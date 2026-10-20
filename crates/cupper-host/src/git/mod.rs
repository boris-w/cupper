//! The git service (design 5.2.10, ADR-10). The host runs git as itself,
//! outside every agent sandbox, while agents may have written the repository's
//! config, so its calls fall in three tiers by where git runs, each a type:
//! the snapshot tier on cupper's own GIT_DIR (T061), the repo tier in the
//! user's repository, checked and overridden on every call (T097, T098), and
//! the user tier for the user's own commit and push (T123).
//!
//! Every call runs the absolute git that CLI discovery found, in
//! `<data dir>/git/`, with a fixed environment; nothing is looked up on PATH.

mod cmd;
mod repo;
pub mod rules;
pub mod run;
pub mod stamp;

use std::ffi::{OsStr, OsString};
use std::fs::DirBuilder;
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use cupper_sys::{Command, Env};

pub use cmd::{GitCmd, Repo, RepoOp, Snapshot, SnapshotOp};
pub use repo::{Done, GitError, RepoNotice};
use run::{Output, RunError};

use crate::clock::Clock;
use crate::durations::{DurationKey, Durations};

/// The git the host runs and what every call of it gets.
#[derive(Debug, Clone)]
pub struct GitTool {
    git: PathBuf,
    git_lfs: Option<PathBuf>,
    path: OsString,
    home: Option<PathBuf>,
    ssh_auth_sock: Option<OsString>,
    /// `<data dir>/git/`, git's working directory.
    dir: PathBuf,
    /// Tests keep git off the machine's system configuration.
    #[cfg(test)]
    test_env: Vec<(&'static str, OsString)>,
}

impl GitTool {
    /// `git` and `git_lfs` come from `CliLocator::tool`, `trusted_path` from
    /// `CliLocator::trusted_path` (T054). Creates `<data_dir>/git/` and the
    /// empty hooks directory in it, mode 0700.
    pub fn new(
        git: PathBuf,
        git_lfs: Option<PathBuf>,
        trusted_path: &[PathBuf],
        home: Option<PathBuf>,
        ssh_auth_sock: Option<OsString>,
        data_dir: &Path,
    ) -> io::Result<Self> {
        let dir = data_dir.join("git");
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir.join("empty-hooks"))?;
        let path = std::env::join_paths(trusted_path).map_err(io::Error::other)?;
        Ok(Self {
            git,
            git_lfs,
            path,
            home,
            ssh_auth_sock,
            dir,
            #[cfg(test)]
            test_env: Vec::new(),
        })
    }

    /// Adds `name=value` to git's environment, for tests that keep git off
    /// the machine's own configuration.
    #[cfg(test)]
    pub(crate) fn with_test_env(mut self, name: &'static str, value: &str) -> Self {
        self.test_env.push((name, value.into()));
        self
    }

    /// git-lfs, for the standard LFS filters (T097).
    pub fn git_lfs(&self) -> Option<&Path> {
        self.git_lfs.as_deref()
    }

    pub fn repo_tier(&self, repo: &Path) -> RepoTier<'_> {
        RepoTier {
            tool: self,
            repo: repo.into(),
        }
    }

    pub fn snapshot_tier(&self, git_dir: &Path, work_tree: &Path) -> SnapshotTier<'_> {
        let (git_dir, work_tree) = (git_dir.into(), work_tree.into());
        SnapshotTier {
            tool: self,
            git_dir,
            work_tree,
        }
    }

    /// `PATH` of trusted entries only, `LC_ALL=C` for parsing messages, no
    /// terminal prompts, no pager, the ssh agent; `HOME` where the tier reads
    /// the user's global config. Nothing else.
    fn env(&self, home: bool) -> Env {
        let mut env = Env::new();
        let mut set = |name, value: &OsStr| env.set(name, value).expect("fixed names, paths");
        set("PATH", &self.path);
        for (name, value) in [
            ("LC_ALL", "C"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_PAGER", "cat"),
        ] {
            set(name, value.as_ref());
        }
        if let Some(home) = self.home.as_ref().filter(|_| home) {
            set("HOME", home.as_os_str());
        }
        if let Some(sock) = &self.ssh_auth_sock {
            set("SSH_AUTH_SOCK", sock);
        }
        #[cfg(test)]
        for (name, value) in &self.test_env {
            set(name, value);
        }
        env
    }

    /// The `-c`s both the repo and the snapshot tier always pass: no
    /// fsmonitor, and hooks from an empty directory.
    fn overrides(&self) -> Vec<OsString> {
        let mut hooks = OsString::from("core.hooksPath=");
        hooks.push(self.dir.join("empty-hooks"));
        [
            "-c".into(),
            "core.fsmonitor=false".into(),
            "-c".into(),
            hooks,
        ]
        .into()
    }

    fn prepare<T>(&self, mut cmd: GitCmd<T>, target: &[OsString], env: Env) -> Prepared {
        let mut command = Command::new(&self.git);
        command
            .args(cmd.line(&self.overrides(), target))
            .env(env)
            .current_dir(&self.dir);
        let (input, timeout, read_only) = (cmd.take_input(), cmd.timeout(), cmd.is_read_only());
        Prepared {
            command,
            input,
            timeout,
            read_only,
        }
    }
}

/// git in the user's repository.
#[derive(Debug)]
pub struct RepoTier<'a> {
    tool: &'a GitTool,
    repo: PathBuf,
}

impl RepoTier<'_> {
    /// The command, without the per-call checks of design 5.2.10: only the
    /// repo tier's `run` (T098), which does them, uses it.
    pub fn prepare(&self, cmd: GitCmd<Repo>) -> Prepared {
        let target = ["-C".into(), self.repo.clone().into()];
        self.tool.prepare(cmd, &target, self.tool.env(true))
    }
}

/// git on cupper's own GIT_DIR for a work tree. T061 completes its
/// environment (index, object store, config).
#[derive(Debug)]
pub struct SnapshotTier<'a> {
    tool: &'a GitTool,
    git_dir: PathBuf,
    work_tree: PathBuf,
}

impl SnapshotTier<'_> {
    pub fn prepare(&self, cmd: GitCmd<Snapshot>) -> Prepared {
        let mut env = self.tool.env(false);
        env.set("GIT_DIR", &self.git_dir).expect("a path");
        env.set("GIT_WORK_TREE", &self.work_tree).expect("a path");
        self.tool.prepare(cmd, &[], env)
    }
}

/// A command ready to run.
#[derive(Debug)]
pub struct Prepared {
    pub command: Command,
    pub input: Option<Vec<u8>>,
    pub timeout: DurationKey,
    pub read_only: bool,
}

impl Prepared {
    pub async fn run(self, clock: &dyn Clock, durations: &Durations) -> Result<Output, RunError> {
        let limit = durations.get(self.timeout);
        run::run(clock, self.command, self.input, limit).await
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::clock::ManualClock;
    use crate::driver::probe::testkit::script;
    use crate::durations::{GIT_COMMAND, GIT_WORKTREE_ADD};
    use crate::store::testutil::TempDir;

    /// What the fake git recorded: its environment without the variables
    /// `/bin/sh` sets itself, its arguments, stdin and working directory.
    fn recorded(dir: &Path) -> [String; 4] {
        let read = |name: &str| fs::read_to_string(dir.join(name)).expect("recorded");
        let shell_own = |line: &&str| {
            !["PWD=", "OLDPWD=", "SHLVL=", "_="]
                .iter()
                .any(|p| line.starts_with(p))
        };
        let env = read("env");
        let mut env: Vec<&str> = env.lines().filter(shell_own).collect();
        env.sort();
        let args = read("args").lines().collect::<Vec<_>>().join(" ");
        [
            env.join(" "),
            args,
            read("stdin"),
            read("cwd").trim().into(),
        ]
    }

    #[tokio::test]
    async fn unit_git_cmd_tiers_pass_only_their_environment_and_fixed_arguments() {
        let dir = TempDir::new("git-cmd");
        let out = dir.path().display();
        let body = format!(
            "env > '{out}/env'; printf '%s\\n' \"$@\" > '{out}/args'; /bin/cat > '{out}/stdin'; pwd -P > '{out}/cwd'"
        );
        let git = script(dir.path(), "git", &body);
        let trusted = ["/opt/homebrew/bin", "/usr/bin"].map(PathBuf::from);
        let (home, sock) = (Some("/home/user".into()), Some("/agent.sock".into()));
        let data = dir.path().join("data");
        let tool = GitTool::new(git, None, &trusted, home, sock, &data).expect("tool");
        let (clock, durations) = (ManualClock::new(), Durations::default());
        let cwd = fs::canonicalize(data.join("git")).expect("git's working directory");
        let hooks = format!("core.hooksPath={}/git/empty-hooks", data.display());
        let base = "GIT_PAGER=cat GIT_TERMINAL_PROMPT=0";
        let path = "PATH=/opt/homebrew/bin:/usr/bin SSH_AUTH_SOCK=/agent.sock";

        let cmd = GitCmd::repo(RepoOp::RevParse)
            .config("filter.x.clean", "")
            .args(["HEAD"]);
        let prepared = tool.repo_tier(Path::new("/repo")).prepare(cmd);
        assert!(prepared.read_only && prepared.timeout == GIT_COMMAND);
        prepared.run(&clock, &durations).await.expect("runs");
        let args = format!(
            "--no-pager --no-optional-locks -c core.quotepath=false -c filter.x.clean= \
             -c core.fsmonitor=false -c {hooks} -C /repo rev-parse HEAD"
        );
        let env = format!("{base} HOME=/home/user LC_ALL=C {path}");
        let want = [env, args, String::new(), cwd.display().to_string()];
        assert_eq!(recorded(dir.path()), want);

        // A write: no `--no-optional-locks`; its input arrives on stdin.
        let cmd = GitCmd::repo(RepoOp::Stage).input(b"the patch".to_vec());
        let prepared = tool.repo_tier(Path::new("/repo")).prepare(cmd);
        assert!(!prepared.read_only);
        prepared.run(&clock, &durations).await.expect("runs");
        let [_, args, stdin, _] = recorded(dir.path());
        let want = format!(
            "--no-pager -c core.quotepath=false -c core.fsmonitor=false -c {hooks} \
             -C /repo apply --whitespace=nowarn --cached"
        );
        assert_eq!((args, stdin.as_str()), (want, "the patch"));

        // The snapshot tier: its GIT_DIR and work tree, no HOME, no `-C`.
        let tier = tool.snapshot_tier(Path::new("/data/git/ws"), Path::new("/work"));
        tier.prepare(GitCmd::snapshot(SnapshotOp::LsFiles))
            .run(&clock, &durations)
            .await
            .expect("runs");
        let [env, args, ..] = recorded(dir.path());
        let want = format!("GIT_DIR=/data/git/ws {base} GIT_WORK_TREE=/work LC_ALL=C {path}");
        assert_eq!(env, want);
        let want = format!(
            "--no-pager --no-optional-locks -c core.quotepath=false -c core.fsmonitor=false \
             -c {hooks} ls-files -z --cached --others --exclude-standard"
        );
        assert_eq!(args, want);

        // `worktree add` gets the long timeout; reads have `-z` where git has it.
        let add = GitCmd::repo(RepoOp::WorktreeAdd);
        assert_eq!(
            (add.is_read_only(), add.timeout()),
            (false, GIT_WORKTREE_ADD)
        );
        let config = GitCmd::repo(RepoOp::ConfigList).line(&[], &[]);
        assert_eq!(
            config.last().map(OsString::as_os_str),
            Some(OsStr::new("-z"))
        );
    }
}
