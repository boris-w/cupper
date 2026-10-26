//! The repo tier's guarded call (design 5.2.10, "仓库档：每次调用前读配置"
//! 1–6). Every git call in the user's repository goes through
//! [`RepoTier::run`]: it reads the configuration first, refuses sources an
//! agent could have written, empties every program the configuration names,
//! never retries without the overrides, and compares the fingerprint of the
//! executable configuration around the call. The caller writes the notices
//! to the session stream.

use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use cupper_protocol::error::{ErrorCode, ErrorReason};
use cupper_protocol::item::NoticeCode;

use super::rules::{self, Bounds, Entry, Setup, Unsafe};
use super::run::{Output, RunError};
use super::stamp::{Fingerprint, Layout};
use super::{GitCmd, Repo, RepoOp, RepoTier};
use crate::clock::Clock;
use crate::diag::redact;
use crate::durations::Durations;

/// How much of the end of git's stderr a [`GitError::Failed`] keeps.
const STDERR_TAIL: usize = 2048;

/// A notice for the caller to write to the session stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoNotice {
    pub code: NoticeCode,
    pub paths: Vec<PathBuf>,
}

/// A call that ran; with a notice when a write changed the configuration.
#[derive(Debug)]
pub struct Done {
    pub output: Output,
    pub notice: Option<RepoNotice>,
}

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("{0} is not a git work tree")]
    NotARepo(PathBuf, #[source] io::Error),
    #[error("the repository's configuration is unsafe")]
    Unsafe(#[from] Unsafe),
    /// Filters with `required=true` lost their programs, and git failed.
    #[error("required filters {0:?} cannot run")]
    RequiredFilter(Vec<String>),
    /// A read whose result is dropped: the configuration changed meanwhile.
    #[error("the repository's configuration changed during the call")]
    ConfigChanged(Vec<PathBuf>),
    /// git failed on its own; `stderr` is the redacted end of what it said.
    #[error("git failed ({status}): {stderr}")]
    Failed {
        status: ExitStatus,
        stderr: String,
        changed: Vec<PathBuf>,
    },
    #[error("cannot run git")]
    Run(#[from] RunError),
    #[error("cannot take the fingerprint of the configuration")]
    Fingerprint(#[source] io::Error),
}

impl GitError {
    pub fn reason(&self) -> Option<ErrorReason> {
        match self {
            Self::NotARepo(..) => Some(ErrorReason::NotARepo),
            Self::Unsafe(_) | Self::RequiredFilter(_) => Some(ErrorReason::UnsafeRepoConfig),
            Self::ConfigChanged(_) => Some(ErrorReason::RepoConfigChanged),
            _ => None,
        }
    }

    /// Why the repo tier refused to run git, and the file or key in the
    /// workspace it is about, for `git.unsafe_repo_config` (design 5.8.6).
    pub fn refusal(&self) -> Option<(&'static str, Option<String>)> {
        let shown = |path: &PathBuf| Some(path.display().to_string());
        Some(match self {
            Self::Unsafe(Unsafe::Writable(path)) => ("writable", shown(path)),
            Self::Unsafe(Unsafe::Unresolved(path)) => ("unresolved", shown(path)),
            Self::Unsafe(Unsafe::Origin(origin)) => ("origin", Some(origin.clone())),
            Self::Unsafe(Unsafe::Unnullable(key)) => ("unnullable", Some(key.clone())),
            Self::Unsafe(Unsafe::Malformed) => ("malformed", None),
            Self::RequiredFilter(filters) => ("required_filter", Some(filters.join(","))),
            _ => return None,
        })
    }

    /// `error.data.code`: the reason's, `timeout`, or `git_error`.
    pub fn code(&self) -> ErrorCode {
        match (self.reason().and_then(|reason| reason.code()), self) {
            (Some(code), _) => code,
            (None, Self::Run(RunError::Timeout)) => ErrorCode::Timeout,
            (None, _) => ErrorCode::GitError,
        }
    }

    /// The notice the caller writes, if any (rules 2, 5 and 6).
    pub fn notice(&self) -> Option<RepoNotice> {
        let (code, paths) = match self {
            Self::Unsafe(Unsafe::Writable(path) | Unsafe::Unresolved(path)) => {
                (NoticeCode::UnsafeRepoConfig, vec![path.clone()])
            }
            Self::Unsafe(_) | Self::RequiredFilter(_) => (NoticeCode::UnsafeRepoConfig, Vec::new()),
            Self::ConfigChanged(paths) | Self::Failed { changed: paths, .. }
                if !paths.is_empty() =>
            {
                (NoticeCode::RepoConfigChanged, paths.clone())
            }
            _ => return None,
        };
        Some(RepoNotice { code, paths })
    }
}

impl RepoTier<'_> {
    /// Runs `cmd` in the repository, guarded. `writable`: every project and
    /// every session's workspace, where configuration must not come from.
    pub async fn run(
        &self,
        clock: &dyn Clock,
        durations: &Durations,
        writable: &[PathBuf],
        cmd: GitCmd<Repo>,
    ) -> Result<Done, GitError> {
        // One spelling of every path, so that a file has one fingerprint entry.
        let not_a_repo = |err| GitError::NotARepo(self.repo.clone(), err);
        let repo = self.repo.canonicalize().map_err(not_a_repo)?;
        let layout = Layout::read(&repo).map_err(not_a_repo)?;
        let listing = self.prepare(GitCmd::repo(RepoOp::ConfigList));
        let listing = listing.run(clock, durations).await?;
        if !listing.status.success() {
            return Err(failed(&listing, Vec::new()));
        }
        let mut entries = rules::parse(&listing.stdout, &repo)?;
        let (git_dir, common_dir) = (&layout.git_dir, &layout.common_dir);
        rules::check_sources(
            &entries,
            &Bounds {
                git_dir,
                common_dir,
                writable,
            },
        )?;
        let hooks = self.tool.dir.join("empty-hooks");
        // No repo tier command makes patches.
        let setup = Setup {
            hooks: &hooks,
            lfs: self.tool.git_lfs(),
            patch: false,
        };
        let overrides = rules::overrides(&entries, &setup)?;
        expand_home(&mut entries, self.tool.home.as_deref());
        let fingerprint = || Fingerprint::take(&layout, &entries).map_err(GitError::Fingerprint);
        let before = fingerprint()?;
        let prepared = self.prepare(cmd.global_args(overrides.global));
        let read_only = prepared.read_only;
        let output = prepared.run(clock, durations).await?;
        let changed = before.changed(&fingerprint()?);
        if read_only && !changed.is_empty() {
            return Err(GitError::ConfigChanged(changed));
        }
        if !output.status.success() {
            return Err(match overrides.required.is_empty() {
                true => failed(&output, changed),
                false => GitError::RequiredFilter(overrides.required),
            });
        }
        let code = NoticeCode::RepoConfigChanged;
        let notice = (!changed.is_empty()).then_some(RepoNotice {
            code,
            paths: changed,
        });
        Ok(Done { output, notice })
    }
}

/// `core.hooksPath` from `~/` expanded as git does, so that the fingerprint
/// covers that directory.
fn expand_home(entries: &mut [Entry], home: Option<&Path>) {
    let Some(home) = home else {
        return;
    };
    for entry in entries.iter_mut().filter(|e| e.key == b"core.hookspath") {
        if let Some(rest) = entry.value.as_deref().and_then(|v| v.strip_prefix(b"~/")) {
            entry.value = Some([home.as_os_str().as_bytes(), b"/", rest].concat());
        }
    }
}

fn failed(output: &Output, changed: Vec<PathBuf>) -> GitError {
    let start = output.stderr.len().saturating_sub(STDERR_TAIL);
    let tail = String::from_utf8_lossy(&output.stderr[start..]);
    let stderr = redact::credentials(tail.trim_end()).into_owned();
    GitError::Failed {
        status: output.status,
        stderr,
        changed,
    }
}

#[cfg(test)]
pub(super) mod tests;
