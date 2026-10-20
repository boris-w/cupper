//! The `projects` and `sessions` rows the index starts from, and the
//! reconciliation at every start (rulings before T034): what the index
//! stream's view lacks or holds differently from the tables is written
//! again, so a checkpoint given up with its log pruned (T026) leaves nothing
//! missing. The sessions' summaries are the status core's to write
//! ([`super::status::Status::load`]).

use std::collections::BTreeMap;
use std::path::PathBuf;

use cupper_protocol::annotations::{TimestampMs, WirePath};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{ProjectId, RequestId, SessionId};
use cupper_protocol::index::{
    HostUpdated, PendingKind, ProjectRemoved, ProjectUpserted, RemovalReason, SessionRemoved,
    SessionStatus, SessionSummary,
};
use cupper_protocol::view::IndexView;
use rusqlite::Error::FromSqlConversionFailure;
use rusqlite::types::Type;
use rusqlite::{Connection, Row};
use serde::de::DeserializeOwned;

use super::status::Loaded;
use crate::trust::{Manifest, Trust, Trusted, TrustedBy};

/// A project as the index keeps it: its row.
#[derive(Debug, Clone)]
pub(super) struct Project {
    pub id: ProjectId,
    pub name: String,
    pub path: PathBuf,
    pub git_common_dir: Option<String>,
    pub added_at: u64,
    pub position: u32,
    pub trust: Trust,
}

impl Project {
    pub fn upserted(&self) -> ProjectUpserted {
        ProjectUpserted {
            project_id: self.id.clone(),
            name: self.name.clone(),
            path: WirePath(self.path.to_string_lossy().into_owned()),
            is_git: self.git_common_dir.is_some(),
            added_at: TimestampMs(self.added_at),
            position: self.position,
            trust_state: self.trust.state.clone(),
        }
    }
}

/// What the tables hold that the index shows.
#[derive(Debug, Default)]
pub(super) struct Tables {
    pub projects: BTreeMap<ProjectId, Project>,
    pub sessions: Vec<Loaded>,
    /// Every session's workspace: like the projects, never a source of git
    /// configuration (design 5.2.10).
    pub workspaces: Vec<PathBuf>,
}

pub(super) fn read(conn: &Connection) -> rusqlite::Result<Tables> {
    let mut tables = Tables::default();
    let mut select = conn.prepare(
        "SELECT project_id, name, path, git_common_dir, added_at, position, trust_state,
                trust_digest, trust_manifest, trusted_at, trusted_by
         FROM projects WHERE removed_at IS NULL",
    )?;
    for project in select.query_map([], project)? {
        let project = project?;
        tables.projects.insert(project.id.clone(), project);
    }
    let mut select = conn.prepare(
        "SELECT session_id, project_id, agent, coalesce(title, ''), title_source,
                workspace_kind, worktree_branch, state, last_turn_end_seq, last_read_seq,
                updated_at, read_only, workspace_path,
                (SELECT count(*) FROM interactions i
                 WHERE i.session_id = s.session_id AND i.status = 'pending'),
                (SELECT kind || ' ' || request_id FROM interactions i
                 WHERE i.session_id = s.session_id AND i.status = 'pending'
                 ORDER BY created_at LIMIT 1)
         FROM sessions s WHERE archived_at IS NULL",
    )?;
    for session in select.query_map([], |row| Ok((session(row)?, row.get(12)?)))? {
        let (session, workspace): (Loaded, String) = session?;
        tables.sessions.push(session);
        tables.workspaces.push(workspace.into());
    }
    Ok(tables)
}

fn project(row: &Row) -> rusqlite::Result<Project> {
    let trusted = match row.get::<_, Option<String>>(7)? {
        None => None,
        Some(digest) => {
            let manifest: Option<String> = row.get(8)?;
            let by: String = row.get(10)?;
            Some(Trusted {
                digest: parsed(7, &digest, str::parse)?,
                manifest: parsed(8, manifest.as_deref().unwrap_or("{}"), Manifest::from_json)?,
                at_ms: row.get::<_, Option<u64>>(9)?.unwrap_or_default(),
                by: parsed(10, &by, |by| TrustedBy::parse(by).ok_or("trusted_by"))?,
            })
        }
    };
    Ok(Project {
        id: parsed(0, &row.get::<_, String>(0)?, str::parse)?,
        name: row.get(1)?,
        path: row.get::<_, String>(2)?.into(),
        git_common_dir: row.get(3)?,
        added_at: row.get(4)?,
        position: row.get(5)?,
        trust: Trust {
            state: text(row, 6)?,
            trusted,
        },
    })
}

/// A session as its row has it; the status core computes its status.
fn session(row: &Row) -> rusqlite::Result<Loaded> {
    let state: String = row.get(7)?;
    let oldest = match row.get::<_, Option<String>>(14)? {
        Some(oldest) => {
            let (kind, request) = oldest.split_once(' ').unwrap_or_default();
            let kind = match kind {
                "question" | "auth_url" => PendingKind::Input,
                _ => PendingKind::Approval,
            };
            Some((kind, parsed(14, request, str::parse::<RequestId>)?))
        }
        None => None,
    };
    let summary = SessionSummary {
        session_id: parsed(0, &row.get::<_, String>(0)?, str::parse::<SessionId>)?,
        project_id: parsed(1, &row.get::<_, String>(1)?, str::parse::<ProjectId>)?,
        agent: text(row, 2)?,
        title: row.get(3)?,
        title_source: text(row, 4)?,
        workspace_kind: text(row, 5)?,
        branch: row.get(6)?,
        status: SessionStatus::Idle,
        unread: false,
        pending_interactions: row.get(13)?,
        pending_kind: None,
        last_activity_at: TimestampMs(row.get(10)?),
        error_code: None,
        read_only: row.get(11)?,
    };
    Ok(Loaded {
        summary,
        running: state == "running",
        ended: row.get(8)?,
        read: row.get(9)?,
        failed: state == "error",
        oldest,
    })
}

/// An open enum stored as its string.
fn text<T: DeserializeOwned>(row: &Row, column: usize) -> rusqlite::Result<T> {
    let value: String = row.get(column)?;
    parsed(column, &value, |value| {
        serde_json::from_value(serde_json::Value::String(value.to_owned()))
    })
}

fn parsed<T, E>(
    column: usize,
    value: &str,
    parse: impl FnOnce(&str) -> Result<T, E>,
) -> rusqlite::Result<T>
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    parse(value).map_err(|err| FromSqlConversionFailure(column, Type::Text, err.into()))
}

/// The events that bring `view` to what the tables and `host` say, but the
/// sessions' summaries.
pub(super) fn reconcile(view: &IndexView, tables: &Tables, host: &HostUpdated) -> Vec<EventBody> {
    let mut bodies = Vec::new();
    for project in tables.projects.values() {
        let upserted = project.upserted();
        if view.projects.get(&project.id) != Some(&upserted) {
            bodies.push(EventBody::ProjectUpserted(upserted));
        }
    }
    for id in view.projects.keys() {
        if !tables.projects.contains_key(id) {
            let project_id = id.clone();
            bodies.push(EventBody::ProjectRemoved(ProjectRemoved { project_id }));
        }
    }
    let seen: Vec<_> = tables
        .sessions
        .iter()
        .map(|s| &s.summary.session_id)
        .collect();
    for id in view.sessions.keys().filter(|id| !seen.contains(id)) {
        bodies.push(EventBody::SessionRemoved(SessionRemoved {
            session_id: id.clone(),
            reason: RemovalReason::Archived,
        }));
    }
    if view.host.as_ref() != Some(host) {
        bodies.push(EventBody::HostUpdated(host.clone()));
    }
    bodies
}
