//! A session loaded from the store when a call comes for it after a
//! restart (design 5.2.6, 5.2.13; T089): its row, queue, last turn and ended
//! requests, and its stream's view, made into the actor's parts and the
//! core's [`Restored`]. What a view rebuilt from turn snapshots lacks is
//! written again from the rows and the agent's cache (design 5.2.8; T169).

use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use cupper_protocol::annotations::{TimestampMs, WirePath};
use cupper_protocol::capabilities::{ConfigOption, ConfigOptionId};
use cupper_protocol::domain::{AgentKind, ClientRole, PrincipalKind};
use cupper_protocol::error::{ErrorCode, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{CommandId, PrincipalId, QueueId, SessionId};
use cupper_protocol::interaction::InteractionResolved;
use cupper_protocol::item::{Item, ToolKind};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::session::{
    PauseReason, ProjectConfig, QueueEntry, QueuePause, QueueUpdated, SessionConfigured,
    SessionWorkspace,
};
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::{Entry, RequestView};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row};
use serde::de::DeserializeOwned;

use super::actor::{Parts, Start};
use super::core::{Input, Restored, SessionCore};
use super::manager::{Counters, cached};
use super::{SessionDeps, internal, mode_tier, not_found};
use crate::driver::api::{AgentSessionId, ProjectConfigPolicy};
use crate::gateway::principal_of;
use crate::orchestrator::queue::{QueueCore, preview};
use crate::orchestrator::turn::{SendMode, SendRequest};
use crate::store::streams::StreamView;

/// What the store has of a session besides its stream.
struct Stored {
    project: String,
    agent: String,
    agent_session: Option<String>,
    path: String,
    config: String,
    set_by: Option<String>,
    confirmed: bool,
    queue_version: u64,
    queue_paused: Option<String>,
    ignored: bool,
    /// The workspace's kind, branch and base, and whether it is read-only.
    workspace: (String, Option<String>, Option<String>, bool),
    /// The project is trusted now.
    trusted: bool,
    /// The first message, until it starts a turn or is queued.
    first: Option<String>,
    /// Each queued message, and the command that queued it.
    queue: Vec<(QueueEntry, Option<CommandId>)>,
    /// The last turn's number, and its end as the startup cleanup recorded it.
    last_turn: Option<(u32, Option<u64>)>,
    ended: Vec<InteractionResolved>,
}

/// Loads session `id` for an actor of its own: `not_found` without its row.
pub(super) async fn load(
    deps: &SessionDeps,
    id: &SessionId,
    (running, pending, queued): Counters,
) -> Result<(Parts, Start), RpcError> {
    let key = id.to_string();
    let stored = deps.readers.read(move |conn| read(conn, &key)).await;
    let stored = stored
        .map_err(internal)?
        .ok_or_else(|| not_found("session"))?;
    let kind = AgentKind::from(stored.agent.as_str());
    let agent = deps.agents.get(&kind).cloned();
    let agent = agent.ok_or_else(|| RpcError::app(ErrorCode::Unavailable, "no such agent"))?;
    let cached = cached(&deps.readers, &kind).await?;
    let stream = (deps.streams.open(&StreamId::Session(id.clone())).await).map_err(internal)?;
    let writer = stream.writer().map_err(internal)?;
    // The view's configuration, capabilities and queue; its running turn
    // with the items that did not end, each with its start and, for a tool
    // call, whether it ran a command; and its pending requests.
    let (configured, capabilities, queue_seen, running_turn, open, expired) =
        stream.read(|view, _| {
            let StreamView::Session(view) = view else {
                unreachable!("a session's stream")
            };
            let running = view.running_turn();
            let items = running.into_iter().flat_map(|t| &t.entries);
            let items = items.filter_map(|entry| match entry {
                Entry::Item(id) => view.items.get(id).filter(|item| item.end.is_none()),
                Entry::Request(_) => None,
            });
            let open: Vec<_> = (items.map(|i| &i.item))
                .map(|i| (i.item_id.clone(), i.started_at, command_of(&i.item)))
                .collect();
            let pending = view.requests.values().filter(|r| r.is_pending());
            (
                view.configured.clone(),
                view.capabilities.clone(),
                view.queue.is_some(),
                running.map(|t| t.started.clone()),
                open,
                pending.map(|r| r.requested.request_id.clone()).collect(),
            )
        });
    // A view rebuilt from turn snapshots lacks what only the view had
    // (design 5.2.8): it is written again from the rows and the cache.
    let mut missing = Vec::new();
    let configured = match configured {
        Some(configured) => configured,
        None => {
            let configured = rebuilt(&stored)?;
            missing.push(EventBody::SessionConfigured(configured.clone()));
            configured
        }
    };
    if let (None, Some(_), Some(caps)) = (&capabilities, &stored.agent_session, &cached) {
        missing.push(EventBody::SessionCapabilities(caps.clone()));
    }

    let paused = stored.queue_paused.as_deref().map(PauseReason::from);
    let project_gate = paused == Some(PauseReason::ProjectConfigChanged);
    let unconfirmed = configured.confirmation_required.clone().map(|c| c.reason);
    let gate = [
        (project_gate, PauseReason::ProjectConfigChanged),
        (unconfirmed.is_some(), PauseReason::ConfigUnconfirmed),
    ];
    let gate = gate.into_iter().filter(|(shut, _)| *shut).map(|(_, r)| r);
    // The mode as last configured, by a mapping that has the option.
    let caps = capabilities.or_else(|| cached.clone());
    let (key, config) = (ConfigOptionId::PermissionMode, &configured.config);
    let mode = caps.as_ref().and_then(|caps| {
        let option = caps.config.iter().find(|o| o.id == key)?;
        let current = config.get(key.as_str())?.clone();
        mode_tier(&[ConfigOption {
            current,
            ..option.clone()
        }])
    });
    let (last_turn_no, ended_at) = stored.last_turn.unwrap_or_default();
    let ended_at = TimestampMs(ended_at.unwrap_or_else(|| deps.clock.now_wall_ms()));
    let (entries, commands): (Vec<_>, Vec<_>) = stored.queue.into_iter().unzip();
    let commands = (entries.iter().zip(commands))
        .filter_map(|(entry, command)| Some((entry.queue_id.clone(), command?)))
        .collect::<HashMap<QueueId, CommandId>>();
    if !queue_seen && stored.queue_version > 0 {
        let (version, entries) = (stored.queue_version, entries.clone());
        let paused = paused.clone().map(|reason| QueuePause { reason });
        let queue = QueueUpdated {
            version,
            paused,
            entries,
        };
        missing.push(EventBody::QueueUpdated(queue));
    }
    // Ended as the host ends a turn's open items (the coalescer): tool calls
    // `abandoned`, a command maybe still running unless interrupts stop them.
    let stops = (caps.as_ref()).is_some_and(|c| c.turn.interrupt_stops_commands);
    let open = (open.into_iter())
        .map(|(item, started, command)| (item, started, command.map(|c| c && !stops)))
        .collect();
    let setter = setter(stored.set_by.as_deref(), stored.confirmed);
    // The first message, when the host stopped before it started a turn.
    let first = (stored.first.as_deref()).filter(|_| stored.last_turn.is_none());
    let first = (first.map(serde_json::from_str).transpose()).map_err(internal)?;
    let first = first.map(|content| SendRequest {
        content,
        mode: SendMode::Queue,
        by: setter.id.clone(),
        reply_to: None,
    });
    let restored = Restored {
        agent_session: stored.agent_session.map(AgentSessionId),
        last_turn_no,
        queue: QueueCore::restored(stored.queue_version, paused, entries),
        gate: gate.collect(),
        mode,
        unconfirmed,
        ended: stored.ended,
        expired,
        interrupted: running_turn.map(|t| (t.turn_id, t.started_at, ended_at)),
        missing,
        open,
        first,
    };
    let initial = restored.first.is_some();

    let setter = (setter, stored.confirmed);
    let (ids, durations) = (Arc::clone(&deps.ids), &deps.durations);
    let mut core = SessionCore::new(id.clone(), ids, durations, false, setter);
    core.require_trust_checks();
    let parts = Parts {
        id: id.clone(),
        core,
        clock: Arc::clone(&deps.clock),
        stream,
        writer,
        readers: deps.readers.clone(),
        agent,
        cwd: PathBuf::from(stored.path),
        config: serde_json::from_str(&stored.config).map_err(internal)?,
        project_config: match configured.project_config {
            ProjectConfig::Loaded => ProjectConfigPolicy::Load,
            _ => ProjectConfigPolicy::Ignore,
        },
        configured,
        cached,
        running,
        pending,
        queued,
        audit: deps.audit.clone(),
        index: Arc::clone(&deps.index),
        project: stored.project.parse().map_err(internal)?,
        git: deps.git.clone(),
        durations: deps.durations.clone(),
        quiescence: deps.quiescence.clone(),
        blobs: deps.blobs.clone(),
    };
    let start = Start {
        first: Input::Restored(Box::new(restored)),
        commands,
        project_gate,
        ignore_for_good: stored.ignored,
        initial,
    };
    Ok((parts, start))
}

/// Who set a loaded session's tier, by `sessions.mode_set_by`: the
/// principal its ID stands for at the handshake. The desktop's two roles
/// share one ID; it counts as the control role only once full access was
/// confirmed, which only that role can. An ID the host does not know counts
/// as the lowest ceiling, `normal` (the ruling on T089: when the setter's
/// origin is unknown, the session needs confirmation).
fn setter(id: Option<&str>, confirmed: bool) -> Principal {
    let role = if confirmed {
        ClientRole::Control
    } else {
        ClientRole::Window
    };
    let found = match id {
        Some("pr_desktop") => principal_of(PrincipalKind::DesktopUi, Some(role)).ok(),
        Some("pr_cli") => principal_of(PrincipalKind::Cli, None).ok(),
        // Only in a build with the test harness.
        Some("pr_test") => principal_of(PrincipalKind::TestHarness, None).ok(),
        _ => None,
    };
    found.unwrap_or_else(|| Principal {
        id: (id.and_then(|id| id.parse().ok()))
            .unwrap_or_else(|| "pr_unknown".parse().expect("a well-formed principal id")),
        kind: PrincipalKind::RemoteDevice,
        role: None,
        scopes: Vec::new(),
    })
}

/// `session.configured` as the row has it, for a view without one. Whether
/// the project's configuration loads follows the project's trust now, and
/// the guard's first check decides any confirmation again.
fn rebuilt(stored: &Stored) -> Result<SessionConfigured, RpcError> {
    let (kind, branch, base_ref, read_only) = stored.workspace.clone();
    let workspace = SessionWorkspace {
        kind: kind.as_str().into(),
        path: WirePath(stored.path.clone()),
        branch,
        base_ref,
    };
    Ok(SessionConfigured {
        agent: stored.agent.as_str().into(),
        workspace,
        config: serde_json::from_str(&stored.config).map_err(internal)?,
        read_only,
        project_config: match stored.trusted && !stored.ignored {
            true => ProjectConfig::Loaded,
            false => ProjectConfig::Ignored,
        },
        confirmation_required: None,
    })
}

/// For a tool call or its late output, whether it ran a command; `None` for
/// any other item.
fn command_of(item: &Item) -> Option<bool> {
    match item {
        Item::ToolCall(call) => Some(call.tool_kind == ToolKind::Execute),
        Item::LateToolOutput(late) => Some(late.of.command.is_some()),
        _ => None,
    }
}

fn read(conn: &Connection, session: &str) -> rusqlite::Result<Option<Stored>> {
    let row = conn.query_row(
        "SELECT project_id, agent, agent_session_id, workspace_path, config_json, mode_set_by,
             full_access_confirmed_at IS NOT NULL, queue_version, queue_paused,
             project_config_ignored, workspace_kind, worktree_branch, base_ref, read_only,
             (SELECT trust_state = 'trusted' FROM projects p
              WHERE p.project_id = sessions.project_id),
             initial_prompt_json
         FROM sessions WHERE session_id = ?1",
        [session],
        |r| {
            Ok(Stored {
                project: r.get(0)?,
                agent: r.get(1)?,
                agent_session: r.get(2)?,
                path: r.get(3)?,
                config: r.get(4)?,
                set_by: r.get(5)?,
                confirmed: r.get(6)?,
                queue_version: r.get(7)?,
                queue_paused: r.get(8)?,
                ignored: r.get(9)?,
                workspace: (r.get(10)?, r.get(11)?, r.get(12)?, r.get(13)?),
                trusted: r.get::<_, Option<bool>>(14)?.unwrap_or(false),
                first: r.get(15)?,
                queue: Vec::new(),
                last_turn: None,
                ended: Vec::new(),
            })
        },
    );
    let Some(mut stored) = row.optional()? else {
        return Ok(None);
    };
    let mut select = conn.prepare(
        "SELECT queue_id, content_json, reply_to, created_at, principal, command_id
         FROM queue_entries WHERE session_id = ?1 ORDER BY position",
    )?;
    let queue = select.query_map([session], |r| {
        let content = json(r, 1)?;
        let entry = QueueEntry {
            queue_id: parsed(r, 0)?,
            preview: preview(&content),
            content,
            reply_to: r
                .get::<_, Option<String>>(2)?
                .and_then(|id| id.parse().ok()),
            created_at: TimestampMs(r.get(3)?),
            created_by: parsed::<PrincipalId>(r, 4)?,
        };
        Ok((entry, r.get::<_, String>(5)?.parse().ok()))
    })?;
    stored.queue = queue.collect::<rusqlite::Result<_>>()?;
    stored.last_turn = conn
        .query_row(
            "SELECT turn_no, ended_at FROM turns WHERE session_id = ?1
             ORDER BY turn_no DESC LIMIT 1",
            [session],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let mut select = conn.prepare(
        "SELECT request_id, status, outcome_json, resolved_by, reason
         FROM interactions WHERE session_id = ?1 AND status != 'pending'",
    )?;
    let ended = select.query_map([session], |r| resolved(r, 0))?;
    stored.ended = ended.collect::<rusqlite::Result<_>>()?;
    Ok(Some(stored))
}

/// Request `request` of session `session` as `interactions` has it, for
/// `interaction/get` once the view let go of it.
pub(super) fn request(
    conn: &Connection,
    session: &str,
    request: &str,
) -> rusqlite::Result<Option<RequestView>> {
    conn.query_row(
        "SELECT request_json, request_id, status, outcome_json, resolved_by, reason
         FROM interactions WHERE request_id = ?1 AND session_id = ?2",
        [request, session],
        |r| {
            let (requested, resolved) = (json(r, 0)?, Some(resolved(r, 1)?));
            Ok(RequestView {
                requested,
                resolved,
            })
        },
    )
    .optional()
}

/// How a request ended, from `interactions`' `request_id`, `status`,
/// `outcome_json`, `resolved_by` and `reason`, from column `at` on.
fn resolved(r: &Row, at: usize) -> rusqlite::Result<InteractionResolved> {
    Ok(InteractionResolved {
        request_id: parsed(r, at)?,
        status: r.get::<_, String>(at + 1)?.as_str().into(),
        outcome: optional(r, at + 2)?,
        resolved_by: optional(r, at + 3)?,
        reason: (r.get::<_, Option<String>>(at + 4)?).map(|reason| reason.as_str().into()),
    })
}

/// Column `i` as JSON of `T`.
fn json<T: DeserializeOwned>(r: &Row, i: usize) -> rusqlite::Result<T> {
    serde_json::from_str(&r.get::<_, String>(i)?).map_err(|e| failed(i, e))
}

fn optional<T: DeserializeOwned>(r: &Row, i: usize) -> rusqlite::Result<Option<T>> {
    match r.get::<_, Option<String>>(i)? {
        Some(_) => json(r, i).map(Some),
        None => Ok(None),
    }
}

fn parsed<T: FromStr>(r: &Row, i: usize) -> rusqlite::Result<T>
where
    T::Err: Error + Send + Sync + 'static,
{
    r.get::<_, String>(i)?.parse().map_err(|e| failed(i, e))
}

fn failed(i: usize, err: impl Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(i, Type::Text, Box::new(err))
}
