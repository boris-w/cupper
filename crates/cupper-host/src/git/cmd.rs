//! [`GitCmd`]: one git command of one tier (design 5.2.10 "调用约定").
//!
//! The tier is a type parameter, and only the tier's own entry builds its
//! commands, so a command cannot cross tiers. The repo tier's entry does not
//! build snapshot commands:
//!
//! ```compile_fail
//! use cupper_host::git::{GitCmd, RepoOp, Snapshot};
//! let cmd: GitCmd<Snapshot> = GitCmd::repo(RepoOp::RevParse);
//! ```
//!
//! nor the snapshot tier's entry repo commands:
//!
//! ```compile_fail
//! use cupper_host::git::{GitCmd, Repo, SnapshotOp};
//! let cmd: GitCmd<Repo> = GitCmd::snapshot(SnapshotOp::LsFiles);
//! ```
//!
//! while each entry builds its own:
//!
//! ```
//! use cupper_host::git::{GitCmd, Repo, RepoOp, Snapshot, SnapshotOp};
//! let repo: GitCmd<Repo> = GitCmd::repo(RepoOp::RevParse);
//! let snapshot: GitCmd<Snapshot> = GitCmd::snapshot(SnapshotOp::LsFiles);
//! ```

use std::ffi::OsString;
use std::marker::PhantomData;

use crate::durations::{DurationKey, GIT_COMMAND, GIT_WORKTREE_ADD};

/// The repo tier: git runs in the user's repository, checked and overridden
/// on every call (T097, T098).
#[derive(Debug)]
pub enum Repo {}

/// The snapshot tier: git runs on cupper's own GIT_DIR (T061).
#[derive(Debug)]
pub enum Snapshot {}

/// The repo tier's commands (design 5.2.10): reading refs and config, and
/// writing the user's index and worktrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoOp {
    /// `config --list --show-origin --includes -z`, read before every call.
    ConfigList,
    RevParse,
    ForEachRef,
    RevList,
    SymbolicRef,
    MergeBase,
    CheckRefFormat,
    BranchList,
    /// `apply --cached` of the patch given as input.
    Stage,
    /// `apply --cached -R`.
    Unstage,
    /// `apply -R`: the work tree.
    Revert,
    /// `update-index --refresh`.
    RefreshIndex,
    WorktreeAdd,
    WorktreeLock,
    WorktreeUnlock,
    WorktreeRemove,
    WorktreePrune,
    WorktreeRepair,
}

/// The snapshot tier's commands; T061 adds the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotOp {
    /// The files `@` references offer (design 5.2.11).
    LsFiles,
}

/// A command's fixed words, with `-z` where the command has it.
#[derive(Debug, Clone, Copy)]
struct Spec {
    words: &'static [&'static str],
    read_only: bool,
    timeout: DurationKey,
}

impl RepoOp {
    fn spec(self) -> Spec {
        let (words, read_only): (&'static [&'static str], bool) = match self {
            Self::ConfigList => (
                &["config", "--list", "--show-origin", "--includes", "-z"],
                true,
            ),
            Self::RevParse => (&["rev-parse"], true),
            Self::ForEachRef => (&["for-each-ref"], true),
            Self::RevList => (&["rev-list"], true),
            Self::SymbolicRef => (&["symbolic-ref"], true),
            Self::MergeBase => (&["merge-base"], true),
            Self::CheckRefFormat => (&["check-ref-format"], true),
            Self::BranchList => (&["branch", "--list"], true),
            Self::Stage => (&["apply", "--whitespace=nowarn", "--cached"], false),
            Self::Unstage => (&["apply", "--whitespace=nowarn", "--cached", "-R"], false),
            Self::Revert => (&["apply", "--whitespace=nowarn", "-R"], false),
            Self::RefreshIndex => (&["update-index", "--refresh"], false),
            Self::WorktreeAdd => (&["worktree", "add"], false),
            Self::WorktreeLock => (&["worktree", "lock"], false),
            Self::WorktreeUnlock => (&["worktree", "unlock"], false),
            Self::WorktreeRemove => (&["worktree", "remove"], false),
            Self::WorktreePrune => (&["worktree", "prune"], false),
            Self::WorktreeRepair => (&["worktree", "repair"], false),
        };
        let timeout = match self {
            Self::WorktreeAdd => GIT_WORKTREE_ADD,
            _ => GIT_COMMAND,
        };
        Spec {
            words,
            read_only,
            timeout,
        }
    }
}

impl SnapshotOp {
    fn spec(self) -> Spec {
        let words = match self {
            Self::LsFiles => &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
        };
        Spec {
            words,
            read_only: true,
            timeout: GIT_COMMAND,
        }
    }
}

/// One git command of the tier `T`.
#[derive(Debug)]
pub struct GitCmd<T> {
    spec: Spec,
    config: Vec<(String, String)>,
    args: Vec<OsString>,
    input: Option<Vec<u8>>,
    tier: PhantomData<fn() -> T>,
}

impl GitCmd<Repo> {
    pub fn repo(op: RepoOp) -> Self {
        Self::new(op.spec())
    }
}

impl GitCmd<Snapshot> {
    pub fn snapshot(op: SnapshotOp) -> Self {
        Self::new(op.spec())
    }
}

impl<T> GitCmd<T> {
    fn new(spec: Spec) -> Self {
        let (config, args, input) = (Vec::new(), Vec::new(), None);
        let tier = PhantomData;
        Self {
            spec,
            config,
            args,
            input,
            tier,
        }
    }

    /// Appends arguments after the command's fixed words.
    pub fn args<I: IntoIterator<Item = S>, S: Into<OsString>>(mut self, args: I) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Adds `-c key=value`. The tier's own `-c`s come later, so this cannot
    /// override them.
    pub fn config(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.push((key.into(), value.into()));
        self
    }

    /// What git reads on stdin; without it, stdin is `/dev/null`.
    pub fn input(mut self, input: Vec<u8>) -> Self {
        self.input = Some(input);
        self
    }

    pub fn is_read_only(&self) -> bool {
        self.spec.read_only
    }

    pub fn timeout(&self) -> DurationKey {
        self.spec.timeout
    }

    pub(super) fn take_input(&mut self) -> Option<Vec<u8>> {
        self.input.take()
    }

    /// The arguments after the program: global options, `-c core.quotepath`,
    /// the caller's `-c`s, the tier's `fixed` ones, the `target` (`-C
    /// <repo>`), the command's words and the caller's arguments.
    pub(super) fn line(&self, fixed: &[OsString], target: &[OsString]) -> Vec<OsString> {
        let mut line: Vec<OsString> = vec!["--no-pager".into()];
        if self.spec.read_only {
            line.push("--no-optional-locks".into());
        }
        line.extend(["-c".into(), "core.quotepath=false".into()]);
        for (key, value) in &self.config {
            line.extend(["-c".into(), format!("{key}={value}").into()]);
        }
        line.extend(fixed.iter().chain(target).cloned());
        line.extend(self.spec.words.iter().map(OsString::from));
        line.extend(self.args.iter().cloned());
        line
    }
}
