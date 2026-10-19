//! Git, workspaces and files (design 5.3.4, 5.2.10, 5.2.11; ADR-10): the
//! state and diffs of a session's workspace, hunk operations, commit and
//! push, whether a worktree may be removed, `@` file search, and opening the
//! workspace in another app. All M1b.
//!
//! `scope` of `git/diff` and `target` of the hunk operations are unions of a
//! string or a one-key object, as 5.3.4 writes them; they are closed, since
//! only clients send them and the host rejects what it does not know.

use schemars::{JsonSchema, SchemaGenerator};
use serde::{Deserialize, Serialize};

use super::Empty;
use super::catalog::Catalog;
use super::interaction::InputEvidence;
use crate::annotations::WirePath;
use crate::domain::BlobRef;
use crate::ids::{CommandId, DiffToken, HunkId, ProjectId, SessionId, TurnId};
use crate::item::DiffChange;
use crate::open_enum;
use crate::session::{DiffStat, ExcludedPath, SnapshotStatus};

method! {
    /// `git/status`: branch, upstream and file states of the session's workspace.
    request GitStatus = "git/status" (GitStatusParams) -> GitStatusResult {
        scope: Read,
        idempotency: Natural,
        milestone: M1b,
        errors: [Unsupported / NotARepo, GitError / UnsafeRepoConfig],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitStatusParams {
    pub session_id: SessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitStatusResult {
    /// Absent on a detached HEAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// E.g. `origin/main`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    /// Commits ahead of and behind the upstream; 0 without one.
    pub ahead: u32,
    pub behind: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_progress: Option<GitOperation>,
    /// Changed, staged, untracked and unmerged files; not ignored ones.
    pub files: Vec<GitStatusFile>,
}

open_enum! {
    /// An operation git has started and not finished (ADR-10, rule 3).
    pub enum GitOperation {
        Merge => "merge",
        CherryPick => "cherry_pick",
        Revert => "revert",
        Rebase => "rebase",
        Bisect => "bisect",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitStatusFile {
    /// Relative to the workspace.
    pub path: WirePath,
    /// The source of a rename or copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<WirePath>,
    /// The index against HEAD.
    pub index: GitFileStatus,
    /// The working tree against the index.
    pub worktree: GitFileStatus,
}

open_enum! {
    /// One column of `git status`.
    pub enum GitFileStatus {
        Unmodified => "unmodified",
        Modified => "modified",
        TypeChanged => "type_changed",
        Added => "added",
        Deleted => "deleted",
        Renamed => "renamed",
        Copied => "copied",
        /// In both columns while a merge conflict is unresolved.
        Unmerged => "unmerged",
        /// In both columns.
        Untracked => "untracked",
    }
}

method! {
    /// `git/diff`: the files a range changed, and a token that names this
    /// diff for `git/file_diff` and the hunk operations (design 5.2.10).
    request GitDiff = "git/diff" (GitDiffParams) -> GitDiffResult {
        scope: Read,
        idempotency: Natural,
        milestone: M1b,
        errors: [Unavailable / SnapshotMissing, Unavailable / IndexUnmerged],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitDiffParams {
    pub session_id: SessionId,
    pub scope: DiffScope,
}

/// What `git/diff` compares: `"unstaged"`, `"staged"`, `"last_turn"` or `{"turn": id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiffScope {
    /// The working tree against the index, untracked files included.
    Unstaged,
    /// The index against HEAD.
    Staged,
    /// The latest turn: its start snapshot against its end snapshot, or
    /// against the working tree while it runs.
    LastTurn,
    /// The same for the given turn.
    Turn(TurnId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitDiffResult {
    pub diff_token: DiffToken,
    pub files: Vec<GitDiffFile>,
    /// Turn ranges: what their snapshots left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<DiffSnapshot>,
    pub stat: DiffStat,
    pub caveats: Vec<DiffCaveat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitDiffFile {
    /// Relative to the workspace.
    pub path: WirePath,
    /// The source of a rename, whose `change` is `update`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<WirePath>,
    pub change: DiffChange,
    /// Lines added and deleted; 0 for a binary file.
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
    /// The file has a custom filter the host does not run: it only shows as
    /// changed, without hunks. Present only when true.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub filter_unsupported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DiffSnapshot {
    pub status: SnapshotStatus,
    pub excluded: Vec<ExcludedPath>,
}

open_enum! {
    /// What the UI notes about a diff (design 5.3.4).
    pub enum DiffCaveat {
        /// Turn ranges, always: in a local workspace the change may include
        /// the user's or other sessions' edits (GT-1).
        MixedChanges => "mixed_changes",
        /// The turn has `late_tool_output` items, or ran while commands that
        /// may still run were going: the change may include their effects.
        LateToolEffects => "late_tool_effects",
    }
}

method! {
    /// `git/file_diff`: the hunks of one file of a `git/diff`.
    request GitFileDiff = "git/file_diff" (GitFileDiffParams) -> GitFileDiffResult {
        scope: Read,
        idempotency: Natural,
        milestone: M1b,
        errors: [Conflict / StaleDiff],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitFileDiffParams {
    pub session_id: SessionId,
    pub diff_token: DiffToken,
    /// As `git/diff` listed it.
    pub path: WirePath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitFileDiffResult {
    pub path: WirePath,
    /// Empty for a `filter_unsupported` file.
    pub hunks: Vec<DiffHunk>,
}

/// A unit the hunk operations act on (design 5.2.10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DiffHunk {
    /// Addressed by content: the same change keeps its ID across diffs.
    pub hunk_id: HunkId,
    pub unit: HunkUnit,
    /// `@@ -1,3 +1,4 @@`, or the mode lines of a `mode` unit.
    pub header: String,
    /// Each line with its ` `, `+`, `-` or `\` prefix, without the line end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<String>>,
    /// Instead of `lines` when they exceed the inline limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines_blob: Option<BlobRef>,
}

open_enum! {
    pub enum HunkUnit {
        Hunk => "hunk",
        /// A change of the file mode, a unit of its own.
        Mode => "mode",
        /// Added, deleted, binary, symlink, submodule and LFS files.
        WholeFile => "whole_file",
    }
}

method! {
    /// `git/stage`: `git apply --cached` hunks or files of a `git/diff`.
    request GitStage = "git/stage" (GitApplyParams) -> Empty {
        scope: Write,
        idempotency: Command,
        milestone: M1b,
        errors: [Conflict / StaleDiff, Conflict / OutcomeUnknown, GitError, Forbidden],
    }
}

method! {
    /// `git/unstage`: `git apply --cached -R`.
    request GitUnstage = "git/unstage" (GitApplyParams) -> Empty {
        scope: Write,
        idempotency: Command,
        milestone: M1b,
        errors: [Conflict / StaleDiff, Conflict / OutcomeUnknown, GitError, Forbidden],
    }
}

method! {
    /// `git/revert`: `git apply -R`, which drops the user's changes.
    request GitRevert = "git/revert" (GitApplyParams) -> Empty {
        scope: Write,
        idempotency: Command,
        milestone: M1b,
        errors: [Conflict / StaleDiff, Conflict / OutcomeUnknown, GitError, Forbidden],
    }
}

/// The parameters of `git/stage`, `git/unstage` and `git/revert`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitApplyParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
    /// The `git/diff` that `target` refers to.
    pub diff_token: DiffToken,
    pub target: GitTarget,
    /// Evidence of real input (design 5.7.8); required for `git/revert`
    /// from the `window` role (design 5.2.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputEvidence>,
}

/// What a hunk operation acts on: an object with exactly one of these keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GitTarget {
    /// `{"all": true}`: every file of the diff; `false` is invalid.
    All(bool),
    /// Whole files, relative to the workspace.
    Paths(Vec<WirePath>),
    HunkIds(Vec<HunkId>),
}

method! {
    /// `git/commit`: commit the index, running the repository's hooks and
    /// signing as in a terminal (user tier).
    request GitCommit = "git/commit" (GitCommitParams) -> GitCommitResult {
        scope: Write,
        idempotency: Command,
        milestone: M1b,
        errors: [GitError / HookFailed, GitError / NothingToCommit, Conflict / OutcomeUnknown],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitCommitParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitCommitResult {
    /// The new commit, in hex.
    #[schemars(extend("x-id" = true))]
    pub oid: String,
}

method! {
    /// `git/push`: push the current branch, with the user's credential helpers (user tier).
    request GitPush = "git/push" (GitPushParams) -> GitPushResult {
        scope: Write,
        idempotency: Command,
        milestone: M1b,
        errors: [GitError / AuthFailed, GitError / Rejected],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitPushParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
    /// Push with `--set-upstream`. Present only when true.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub set_upstream: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitPushResult {
    pub remote: String,
    pub branch: String,
}

method! {
    /// `git/branches`: the branches of a project's repository, e.g. to pick a worktree's base.
    request GitBranches = "git/branches" (GitBranchesParams) -> GitBranchesResult {
        scope: Read,
        idempotency: Natural,
        milestone: M1b,
        errors: [],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitBranchesParams {
    pub project_id: ProjectId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitBranchesResult {
    /// Absent on a detached HEAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    pub local: Vec<String>,
    /// Remote-tracking branches, e.g. `origin/main`.
    pub remote: Vec<String>,
}

method! {
    /// `worktree/check`: whether removing the session's worktree could lose
    /// work, by the cleanup rules of ADR-10; read-only.
    request WorktreeCheck = "worktree/check" (WorktreeCheckParams) -> WorktreeCheckResult {
        scope: Read,
        idempotency: Natural,
        milestone: M1b,
        errors: [],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorktreeCheckParams {
    pub session_id: SessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorktreeCheckResult {
    /// `blockers` is empty. With `case_only_dirty`, removing still takes
    /// `session/archive` with `worktree: remove_case_only_dirty`.
    pub removable: bool,
    pub blockers: Vec<WorktreeBlocker>,
    pub warnings: WorktreeWarnings,
}

open_enum! {
    /// Why a worktree is kept: one value per cleanup rule of ADR-10, in its
    /// order. `worktree/check` lists them, and `session/archive` returns them
    /// as `blockers` when it keeps the worktree.
    pub enum WorktreeBlocker {
        /// Rule 1: modified or staged files.
        ModifiedFiles => "modified_files",
        /// Rule 2: untracked files; ignored files do not count.
        UntrackedFiles => "untracked_files",
        /// Rule 3: a merge, cherry-pick, revert, rebase or bisect in progress.
        OperationInProgress => "operation_in_progress",
        /// Rule 4: a lock that cupper did not take.
        ForeignLock => "foreign_lock",
        /// Rule 5: commits on HEAD that no other branch, remote-tracking
        /// branch or tag contains.
        HeadOnlyCommits => "head_only_commits",
        /// Rule 6: commits not pushed yet; not checked when the repository has no remote.
        UnpushedCommits => "unpushed_commits",
        /// Rule 7: initialized submodules.
        InitializedSubmodules => "initialized_submodules",
    }
}

/// Worth telling the user, but no reason to keep the worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorktreeWarnings {
    /// Ignored files, which removing deletes.
    pub ignored_files: u32,
    /// Files show as changed only because their paths differ in case.
    pub case_only_dirty: bool,
}

method! {
    /// `files/search`: fuzzy search of the files of a workspace, for `@` references.
    request FilesSearch = "files/search" (FilesSearchParams) -> FilesSearchResult {
        scope: Read,
        idempotency: Natural,
        milestone: M1b,
        errors: [],
    }
}

/// Exactly one of `session_id` (its workspace) and `project_id` (the project directory).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FilesSearchParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FilesSearchResult {
    /// Best first.
    pub results: Vec<FileMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FileMatch {
    /// Relative to the workspace.
    pub path: WirePath,
    pub score: u32,
    /// The matched parts of `path` as `[start, end)` in UTF-16 code units, for JS.
    pub ranges_utf16: Vec<[u32; 2]>,
}

method! {
    /// `workspace/open`: open the session's workspace in another app (UI-0).
    request WorkspaceOpen = "workspace/open" (WorkspaceOpenParams) -> Empty {
        scope: UiLocal,
        idempotency: Natural,
        milestone: M1b,
        errors: [Unavailable / AppNotFound],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceOpenParams {
    pub session_id: SessionId,
    pub target: OpenTarget,
}

open_enum! {
    pub enum OpenTarget {
        /// The app of the `apps.terminal` setting.
        Terminal => "terminal",
        /// The app of the `apps.editor` setting.
        Editor => "editor",
        Finder => "finder",
    }
}

pub fn register(c: &mut Catalog, g: &mut SchemaGenerator) {
    c.request::<GitStatus>(g);
    c.request::<GitDiff>(g);
    c.request::<GitFileDiff>(g);
    c.request::<GitStage>(g);
    c.request::<GitUnstage>(g);
    c.request::<GitRevert>(g);
    c.request::<GitCommit>(g);
    c.request::<GitPush>(g);
    c.request::<GitBranches>(g);
    c.request::<WorktreeCheck>(g);
    c.request::<FilesSearch>(g);
    c.request::<WorkspaceOpen>(g);
}

#[cfg(test)]
pub(super) mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::methods::testkit::request;
    use crate::testutil::{open_values, roundtrip};

    /// Typical values of every method in this group (design 5.3.4).
    pub fn samples() -> Vec<&'static str> {
        let apply = |target: Value| json!({"command_id": "c-1", "session_id": "s_1", "diff_token": "dt_1", "target": target});
        let (session, hash) = (json!({"session_id": "s_1"}), "ab".repeat(32));
        let a_rs = json!({"path": "src/a.rs", "change": "update", "additions": 3, "deletions": 1,
            "binary": false});
        vec![
            request::<GitStatus>(
                session.clone(),
                json!({"branch": "cupper/fix-login", "upstream": "origin/cupper/fix-login",
                    "ahead": 2, "behind": 0, "in_progress": "rebase", "files": [
                    {"path": "src/a.rs", "index": "modified", "worktree": "unmodified"},
                    {"path": "src/b.rs", "old_path": "src/c.rs", "index": "renamed", "worktree": "modified"},
                    {"path": "notes.md", "index": "untracked", "worktree": "untracked"}]}),
            ),
            request::<GitStatus>(
                session.clone(),
                json!({"ahead": 0, "behind": 0, "files": []}),
            ),
            request::<GitDiff>(
                json!({"session_id": "s_1", "scope": "last_turn"}),
                json!({"diff_token": "dt_1", "files": [a_rs, {"path": "logo.png", "change": "add",
                    "additions": 0, "deletions": 0, "binary": true}, {"path": "secret.env",
                    "change": "update", "additions": 0, "deletions": 0, "binary": false,
                    "filter_unsupported": true}],
                    "snapshot": {"status": "partial", "excluded": [{"path": "data.bin", "reason": "too_large"}]},
                    "stat": {"files": 3, "added": 3, "removed": 1},
                    "caveats": ["mixed_changes", "late_tool_effects"]}),
            ),
            request::<GitDiff>(
                json!({"session_id": "s_1", "scope": {"turn": "t_1"}}),
                json!({"diff_token": "dt_2", "files": [{"path": "src/b.rs", "old_path": "src/c.rs",
                    "change": "update", "additions": 0, "deletions": 0, "binary": false}],
                    "snapshot": {"status": "ok", "excluded": []},
                    "stat": {"files": 1, "added": 0, "removed": 0}, "caveats": ["mixed_changes"]}),
            ),
            request::<GitDiff>(
                json!({"session_id": "s_1", "scope": "staged"}),
                json!({"diff_token": "dt_3", "files": [a_rs],
                    "stat": {"files": 1, "added": 3, "removed": 1}, "caveats": []}),
            ),
            request::<GitFileDiff>(
                json!({"session_id": "s_1", "diff_token": "dt_1", "path": "src/a.rs"}),
                json!({"path": "src/a.rs", "hunks": [
                    {"hunk_id": "hk_1", "unit": "hunk", "header": "@@ -1,2 +1,3 @@",
                        "lines": [" fn a() {", "+    b();", " }"]},
                    {"hunk_id": "hk_2", "unit": "mode", "header": "old mode 100644\nnew mode 100755"},
                    {"hunk_id": "hk_3", "unit": "hunk", "header": "@@ -10,900 +10,950 @@",
                        "lines_blob": {"hash": hash, "size": 70000, "media_type": "text/x-diff"}}]}),
            ),
            request::<GitStage>(apply(json!({"hunk_ids": ["hk_1", "hk_2"]})), json!({})),
            request::<GitUnstage>(apply(json!({"paths": ["src/a.rs"]})), json!({})),
            request::<GitRevert>(
                json!({"command_id": "c-1", "session_id": "s_1", "diff_token": "dt_1",
                    "target": {"all": true}, "input": {"kind": "key", "key": "digit", "digit": 3}}),
                json!({}),
            ),
            request::<GitCommit>(
                json!({"command_id": "c-2", "session_id": "s_1", "message": "Fix the login\n\nDetails."}),
                json!({"oid": "0f".repeat(20)}),
            ),
            request::<GitPush>(
                json!({"command_id": "c-3", "session_id": "s_1", "set_upstream": true}),
                json!({"remote": "origin", "branch": "cupper/fix-login"}),
            ),
            request::<GitPush>(
                json!({"command_id": "c-4", "session_id": "s_1"}),
                json!({"remote": "origin", "branch": "main"}),
            ),
            request::<GitBranches>(
                json!({"project_id": "p_1"}),
                json!({"current": "main", "local": ["main", "cupper/fix-login"],
                    "remote": ["origin/main", "origin/HEAD"]}),
            ),
            request::<WorktreeCheck>(
                session.clone(),
                json!({"removable": false, "blockers": ["untracked_files", "unpushed_commits"],
                    "warnings": {"ignored_files": 12, "case_only_dirty": false}}),
            ),
            request::<WorktreeCheck>(
                session.clone(),
                json!({"removable": true, "blockers": [],
                    "warnings": {"ignored_files": 0, "case_only_dirty": true}}),
            ),
            request::<FilesSearch>(
                json!({"session_id": "s_1", "query": "lgn", "limit": 20}),
                json!({"results": [{"path": "src/login.rs", "score": 120,
                    "ranges_utf16": [[4, 5], [6, 8]]}]}),
            ),
            request::<FilesSearch>(
                json!({"project_id": "p_1", "query": "readme"}),
                json!({"results": []}),
            ),
            request::<WorkspaceOpen>(json!({"session_id": "s_1", "target": "editor"}), json!({})),
        ]
    }

    #[test]
    fn unit_proto_methods_ext_scope_and_target_are_one_of_their_forms() {
        let scopes: Vec<DiffScope> =
            roundtrip(&json!(["unstaged", "staged", "last_turn", {"turn": "t_1"}]));
        assert_eq!(scopes[3], DiffScope::Turn("t_1".parse().unwrap()));
        let targets: Vec<GitTarget> =
            roundtrip(&json!([{"all": true}, {"paths": ["a"]}, {"hunk_ids": ["hk_1"]}]));
        assert_eq!(targets[0], GitTarget::All(true));
        for raw in [json!("everything"), json!({"turn": "s_1"})] {
            assert!(serde_json::from_value::<DiffScope>(raw).is_err());
        }
        let mixed = json!({"all": true, "paths": ["a"]});
        for raw in [json!("all"), mixed, json!({"hunk_ids": ["x_1"]})] {
            assert!(serde_json::from_value::<GitTarget>(raw).is_err());
        }
    }

    #[test]
    fn unit_proto_methods_ext_worktree_blockers_are_the_adr10_rules() {
        // One value per cleanup rule, in the order of ADR-10.
        assert_eq!(
            WorktreeBlocker::KNOWN,
            [
                "modified_files",
                "untracked_files",
                "operation_in_progress",
                "foreign_lock",
                "head_only_commits",
                "unpushed_commits",
                "initialized_submodules",
            ]
        );
        open_values::<WorktreeBlocker>(WorktreeBlocker::KNOWN);
        open_values::<GitOperation>(GitOperation::KNOWN);
        open_values::<GitFileStatus>(GitFileStatus::KNOWN);
        open_values::<DiffCaveat>(DiffCaveat::KNOWN);
        open_values::<HunkUnit>(HunkUnit::KNOWN);
        open_values::<OpenTarget>(OpenTarget::KNOWN);
    }
}
