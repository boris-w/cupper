//! `project/add`, `project/trust` and `project/trust_info` (design 5.5.10,
//! 5.2.6). [`prepare`] does the slow part in a task of its own: the
//! realpath, where the git work tree is, and the trust scan on the blocking
//! pool. The actor then decides, writes the row and the event, and answers.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cupper_protocol::error::{ErrorCode, ErrorReason, RpcError};
use cupper_protocol::methods::project::{
    Finding, ProjectAddResult, ProjectTrustInfoResult, ProjectTrustResult, TrustScan,
    TrustedFileChange,
};
use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::budget;
use super::rows::Project;
use crate::clock::Clock;
use crate::durations::Durations;
use crate::git::{GitCmd, GitError, GitTool, RepoOp};
use crate::trust::{self, Scan, Trust};

/// The project's directory, its git work tree, and the scan of its range.
#[derive(Debug)]
pub(super) struct Prepared {
    pub path: PathBuf,
    /// The work tree's root and the common git dir, both realpaths; `None`
    /// outside git or without a git to run.
    pub git: Option<(PathBuf, String)>,
    pub scan: Scan,
}

/// What [`prepare`] needs, taken from the actor when the call arrives.
#[derive(Debug, Clone)]
pub(crate) struct Context {
    pub git: Option<GitTool>,
    /// Every project and session workspace (design 5.2.10).
    pub writable: Vec<PathBuf>,
    pub clock: Arc<dyn Clock>,
    pub durations: Durations,
}

pub(super) async fn prepare(path: PathBuf, cx: Context) -> Result<Prepared, RpcError> {
    let path = std::fs::canonicalize(&path)
        .ok()
        .filter(|path| path.is_dir())
        .ok_or_else(|| RpcError::app(ErrorCode::NotFound, "no such directory"))?;
    let git = git_of(&path, &cx).await?;
    let (dir, root) = (path.clone(), git.as_ref().map(|(root, _)| root.clone()));
    let scan = tokio::task::spawn_blocking(move || trust::scan(&dir, root.as_deref()))
        .await
        .map_err(|err| RpcError::app(ErrorCode::Internal, err.to_string()))?
        .map_err(|err| {
            let message = format!("{}: {err}", err.code());
            RpcError::app(ErrorCode::Unavailable, message).with_reason(ErrorReason::ScanFailed)
        })?;
    Ok(Prepared { path, git, scan })
}

/// The root and common dir of the git work tree `path` (a realpath) is in;
/// `None` outside git or without a git to run.
pub(crate) async fn git_of(
    path: &Path,
    cx: &Context,
) -> Result<Option<(PathBuf, String)>, RpcError> {
    match (&cx.git, work_tree(path)) {
        (Some(tool), Some(tree)) => Ok(Some(locate(tool, &tree, cx).await?)),
        _ => Ok(None),
    }
}

/// The nearest directory at or above `path` with a `.git`, as git finds it.
fn work_tree(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_owned)
}

/// What git says the work tree's root and common dir are, through the
/// guarded repo tier (T098).
async fn locate(tool: &GitTool, tree: &Path, cx: &Context) -> Result<(PathBuf, String), RpcError> {
    let args = [
        "--path-format=absolute",
        "--show-toplevel",
        "--git-common-dir",
    ];
    let cmd = GitCmd::repo(RepoOp::RevParse).args(args);
    let tier = tool.repo_tier(tree);
    let done = tier.run(&*cx.clock, &cx.durations, &cx.writable, cmd).await;
    let done = done.map_err(|err: GitError| {
        let error = RpcError::app(err.code(), err.to_string());
        err.reason()
            .map_or(error.clone(), |reason| error.with_reason(reason))
    })?;
    let out = String::from_utf8_lossy(&done.output.stdout);
    let mut lines = out.lines();
    match (lines.next(), lines.next()) {
        (Some(root), Some(common)) => Ok((root.into(), common.to_owned())),
        _ => Err(RpcError::app(
            ErrorCode::GitError,
            "git rev-parse said nothing",
        )),
    }
}

/// `projects.repo_key`: the first 16 hex digits of the common dir's sha-256.
pub(super) fn repo_key(common_dir: &str) -> String {
    let hash = Sha256::digest(common_dir.as_bytes());
    hash.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

pub(super) fn insert(project: &Project, conn: &Connection) -> rusqlite::Result<()> {
    let common = project.git_common_dir.as_deref();
    conn.execute(
        "INSERT INTO projects (project_id, path, name, git_common_dir, repo_key, position,
                               added_at, trust_state)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            project.id.as_str(),
            project.path.to_string_lossy(),
            project.name,
            common,
            common.map(repo_key),
            project.position,
            project.added_at,
            project.trust.state.as_str(),
        ],
    )?;
    update(project, conn)
}

/// The `trust_*` columns of `project`.
pub(super) fn update(project: &Project, conn: &Connection) -> rusqlite::Result<()> {
    let trusted = project.trust.trusted.as_ref();
    conn.execute(
        "UPDATE projects SET trust_state = ?2, trust_digest = ?3, trust_manifest = ?4,
                             trusted_at = ?5, trusted_by = ?6
         WHERE project_id = ?1",
        rusqlite::params![
            project.id.as_str(),
            project.trust.state.as_str(),
            trusted.map(|t| t.digest.as_str()),
            trusted.map(|t| t.manifest.to_json()),
            trusted.map(|t| t.at_ms),
            trusted.map(|t| t.by.as_str()),
        ],
    )
    .map(drop)
}

pub(super) fn add_result(project: &Project, scan: &Scan) -> Value {
    let fitted = budget::fit(scan.findings(), None);
    json!(ProjectAddResult {
        project_id: project.id.clone(),
        trust: TrustScan {
            state: project.trust.state.clone(),
            findings: fitted.findings,
            findings_omitted: fitted.findings_omitted,
            trust_digest: scan.digest.clone(),
        },
    })
}

pub(super) fn trust_result(trust: &Trust) -> Value {
    json!(ProjectTrustResult {
        trust_state: trust.state.clone(),
    })
}

pub(super) fn info_result(project: &Project, trust: &Trust, scan: &Scan) -> Value {
    let findings: Vec<Finding> = scan.findings();
    let diff: Option<Vec<TrustedFileChange>> = trust.diff_from_trusted(scan);
    let fitted = budget::fit(findings, diff);
    json!(ProjectTrustInfoResult {
        path: project.upserted().path,
        state: trust.state.clone(),
        findings: fitted.findings,
        findings_omitted: fitted.findings_omitted,
        trust_digest: scan.digest.clone(),
        diff_from_trusted: fitted.diff,
        diff_omitted: fitted.diff_omitted,
    })
}
