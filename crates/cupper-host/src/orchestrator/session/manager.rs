//! `SessionManager` (design 5.2.6): the table of session actors, the
//! gateway's [`SessionPort`], and `session/create`.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cupper_protocol::annotations::WirePath;
use cupper_protocol::capabilities::{ConfigOptionId, SessionCapabilities};
use cupper_protocol::domain::{AgentKind, Tier};
use cupper_protocol::envelope::Cause;
use cupper_protocol::error::{ErrorCode, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::SessionId;
use cupper_protocol::index::{TrustState, WorkspaceKind};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::methods::session::{
    SessionCreate, SessionCreateParams, SessionCreateResult, WorkspaceSpec,
};
use cupper_protocol::rpc::Method;
use cupper_protocol::session::{
    ProjectConfig, SessionConfigured, SessionWorkspace, WorkspaceReady,
};
use cupper_protocol::sync::StreamId;
use futures::FutureExt;
use futures::future::{self, BoxFuture};
use rusqlite::OptionalExtension;
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use super::actor::{Mail, Parts, SessionActor};
use super::core::SessionCore;
use super::rows::Rows;
use super::{internal, not_found, parse};
use crate::clock::{Clock, IdGen};
use crate::driver::api::{AgentPort, ProjectConfigPolicy};
use crate::durations::Durations;
use crate::gateway::{Call, Port, Reply, SessionPort};
use crate::ledger::Outcome;
use crate::orchestrator::queue::preview;
use crate::orchestrator::turn::{SendMode, SendRequest};
use crate::policy::{self, Confirmable, Facts};
use crate::quiescence::{ActorKind, Quiescence};
use crate::store::streams::{Batch, Streams};
use crate::store::{Durability, Readers, Writer};

/// The session actors in the quiescence barrier.
pub const SESSION: ActorKind = ActorKind::new("session");

/// What the sessions need of the host.
#[derive(Debug, Clone)]
pub struct SessionDeps {
    pub streams: Streams,
    pub writer: Writer,
    pub readers: Readers,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<IdGen>,
    pub durations: Durations,
    /// The runtime of each agent the host can run.
    pub agents: HashMap<AgentKind, Arc<dyn AgentPort>>,
    pub quiescence: Quiescence,
}

/// The session manager. Clones share it; the host has one.
#[derive(Debug, Clone)]
pub struct SessionManager {
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    deps: SessionDeps,
    /// Held only to look up, add and count.
    sessions: Mutex<HashMap<SessionId, Handle>>,
}

#[derive(Debug)]
struct Handle {
    mail: mpsc::UnboundedSender<Mail>,
    running: Arc<AtomicBool>,
}

impl SessionManager {
    /// Registers the session actors in the quiescence barrier.
    pub fn new(deps: SessionDeps) -> Self {
        deps.quiescence.register(SESSION);
        let sessions = Mutex::default();
        Self {
            shared: Arc::new(Shared { deps, sessions }),
        }
    }

    fn sessions(&self) -> MutexGuard<'_, HashMap<SessionId, Handle>> {
        self.shared
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// `session/create` (design 5.2.6, 5.5.2, local workspaces): the session
    /// row with its first message, `session.configured`, `workspace.ready`
    /// and the command's result in one transaction; then its actor starts
    /// the first turn.
    async fn create(self, call: Call) -> Outcome {
        let Call {
            principal,
            params,
            ticket,
            ..
        } = call;
        let accepted = match self.accept(&principal, &params).await {
            Ok(accepted) => accepted,
            Err(err) => {
                if let Some(ticket) = ticket {
                    ticket.reject(err.clone());
                }
                return Err(err);
            }
        };
        let Accepted {
            params,
            agent,
            path,
            project_config,
            config,
            cached,
        } = accepted;
        let deps = &self.shared.deps;
        let id = SessionId::from_uuid(deps.ids.new_id().as_u128());
        let stream_id = StreamId::Session(id.clone());
        let stream = deps.streams.open(&stream_id).await.map_err(internal)?;
        let mut writer = stream.writer().map_err(internal)?;
        let workspace = SessionWorkspace {
            kind: WorkspaceKind::Local,
            path: WirePath(path.clone()),
            branch: None,
            base_ref: None,
        };
        let mut current: BTreeMap<String, String> = (cached.iter())
            .flat_map(|caps| caps.config.iter())
            .map(|o| (o.id.to_string(), o.current.clone()))
            .collect();
        current.extend(config.clone());
        let configured = SessionConfigured {
            agent: params.agent.clone(),
            workspace,
            config: current,
            read_only: false,
            project_config: match project_config {
                ProjectConfigPolicy::Load => ProjectConfig::Loaded,
                ProjectConfigPolicy::Ignore => ProjectConfig::Ignored,
            },
            confirmation_required: None,
        };
        let row = Rows::created(
            [
                id.to_string(),
                params.project_id.to_string(),
                params.agent.to_string(),
                path.clone(),
                json!(configured.config).to_string(),
                json!(params.message).to_string(),
                preview(&params.message),
                principal.id.to_string(),
            ],
            deps.clock.now_wall_ms(),
        );
        let result = json!(SessionCreateResult {
            session_id: id.clone()
        });
        let completion = ticket.map(|ticket| ticket.complete(Ok(result.clone())));
        let record = completion.as_ref().map(|c| c.row());
        let ready = WorkspaceReady {
            path: WirePath(path.clone()),
            branch: None,
        };
        let batch = Batch {
            bodies: vec![
                EventBody::SessionConfigured(configured.clone()),
                EventBody::WorkspaceReady(ready),
            ],
            cause: Some(Cause {
                command_id: params.command_id.clone(),
                principal: principal.id.clone(),
            }),
            durability: Durability::Normal,
            with: Some(Box::new(move |conn| {
                row.write(conn)?;
                record.map_or(Ok(()), |record| record(conn))
            })),
        };
        writer.append(batch).await.map_err(internal)?;
        let core = SessionCore::new(Arc::clone(&deps.ids), &deps.durations, false);
        let (mail, inbox) = mpsc::unbounded_channel();
        let running = Arc::new(AtomicBool::new(false));
        let parts = Parts {
            id: id.clone(),
            core,
            clock: Arc::clone(&deps.clock),
            stream,
            writer,
            db: deps.writer.clone(),
            readers: deps.readers.clone(),
            agent,
            cwd: PathBuf::from(path),
            config,
            project_config,
            configured,
            cached,
            running: Arc::clone(&running),
        };
        let first = SendRequest {
            content: params.message,
            mode: SendMode::Queue,
            by: principal.id,
            reply_to: None,
        };
        let participant = deps.quiescence.enter(SESSION);
        tokio::spawn(SessionActor::run(parts, inbox, first, participant));
        self.sessions().insert(id, Handle { mail, running });
        if let Some(completion) = completion {
            completion.committed();
        }
        Ok(result)
    }

    /// The checks before a session is created: the workspace, the agent,
    /// the project and its trust, and the tier of the mode asked for.
    async fn accept(
        &self,
        principal: &Principal,
        params: &serde_json::Value,
    ) -> Result<Accepted, RpcError> {
        let params: SessionCreateParams = parse(params)?;
        if !matches!(params.workspace, WorkspaceSpec::Local) {
            let message = "only local workspaces before M1b";
            return Err(RpcError::app(ErrorCode::Unsupported, message));
        }
        let deps = &self.shared.deps;
        let agent = deps.agents.get(&params.agent).cloned();
        let agent = agent.ok_or_else(|| RpcError::app(ErrorCode::Unavailable, "no such agent"))?;
        let project = params.project_id.to_string();
        let project = deps.readers.read(move |conn| {
            conn.query_row(
                "SELECT path, trust_state FROM projects
                 WHERE project_id = ?1 AND removed_at IS NULL",
                [project],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
        });
        let project = project.await.map_err(internal)?;
        let (path, trust) = project.ok_or_else(|| not_found("project"))?;
        let project_config = match policy::attach(&params.agent, &TrustState::from(trust.as_str()))?
        {
            policy::ProjectConfig::Load => ProjectConfigPolicy::Load,
            policy::ProjectConfig::UserOnly => ProjectConfigPolicy::Ignore,
        };
        let cached = cached(&deps.readers, &params.agent).await?;
        let initial = params.config.clone().unwrap_or_default();
        if let Some(mode) = &initial.permission_mode {
            // A mode the mapping does not know is full access (design 5.6.5).
            let tier = tier_of(cached.as_ref(), mode).unwrap_or(Tier::FullAccess);
            policy::tier_change(principal, None, &tier)?;
        }
        let config = [
            (ConfigOptionId::PermissionMode, initial.permission_mode),
            (ConfigOptionId::Model, initial.model),
            (ConfigOptionId::ReasoningEffort, initial.reasoning_effort),
        ];
        let config = (config.into_iter())
            .filter_map(|(id, value)| Some((id.to_string(), value?)))
            .collect();
        Ok(Accepted {
            params,
            agent,
            path,
            project_config,
            config,
            cached,
        })
    }

    fn running_turns(&self) -> u32 {
        let running = self
            .sessions()
            .values()
            .filter(|h| h.running.load(Ordering::Relaxed))
            .count();
        u32::try_from(running).unwrap_or(u32::MAX)
    }
}

struct Accepted {
    params: SessionCreateParams,
    agent: Arc<dyn AgentPort>,
    path: String,
    project_config: ProjectConfigPolicy,
    config: BTreeMap<String, String>,
    cached: Option<SessionCapabilities>,
}

/// The agent's capabilities in the host's cache (design 5.6.7), if any.
async fn cached(
    readers: &Readers,
    agent: &AgentKind,
) -> Result<Option<SessionCapabilities>, RpcError> {
    let agent = agent.to_string();
    let json = readers.read(move |conn| {
        conn.query_row(
            "SELECT json FROM agent_capabilities WHERE agent = ?1",
            [agent],
            |row| row.get::<_, String>(0),
        )
        .optional()
    });
    let json = json.await.map_err(internal)?;
    Ok(json.and_then(|json| serde_json::from_str(&json).ok()))
}

/// The tier of permission mode `mode` by the agent's mapping.
fn tier_of(capabilities: Option<&SessionCapabilities>, mode: &str) -> Option<Tier> {
    let options = capabilities?.config.iter();
    let option = options
        .into_iter()
        .find(|o| o.id == ConfigOptionId::PermissionMode)?;
    let value = option.values.iter().find(|v| v.value == mode)?;
    Some(value.risk.clone())
}

/// Refuses `call` with `err`.
fn refuse(call: Call, err: RpcError) -> Reply {
    if let Some(ticket) = call.ticket {
        ticket.reject(err.clone());
    }
    Box::pin(future::ready(Err(err)))
}

impl Port for SessionManager {
    /// Queues a session's call in its actor's mailbox; `session/create` runs
    /// in the reply's task. A session without an actor is `not_found` until
    /// T089 loads sessions from the store.
    fn deliver(&self, call: Call) -> Reply {
        if call.method == SessionCreate::NAME {
            return Box::pin(self.clone().create(call));
        }
        let id = parse::<SessionId>(&call.params["session_id"]);
        let mail = id
            .ok()
            .and_then(|id| self.sessions().get(&id).map(|h| h.mail.clone()));
        let Some(mail) = mail else {
            return refuse(call, not_found("session"));
        };
        let (reply, answer) = oneshot::channel();
        if let Err(mpsc::error::SendError((call, _))) = mail.send((call, reply)) {
            return refuse(call, internal("the session stopped"));
        }
        let stopped = || Err(internal("the session stopped before it replied"));
        Box::pin(answer.map(move |outcome| outcome.unwrap_or_else(|_| stopped())))
    }
}

impl SessionPort for SessionManager {
    fn facts(&self, command: &Confirmable) -> BoxFuture<'static, Result<Facts, RpcError>> {
        let running_turns = self.running_turns();
        let Confirmable::Create(params) = command else {
            return Box::pin(future::ready(Ok(Facts {
                running_turns,
                ..Facts::default()
            })));
        };
        let readers = self.shared.deps.readers.clone();
        let agent = params.agent.clone();
        let mode = params
            .config
            .as_ref()
            .and_then(|c| c.permission_mode.clone());
        Box::pin(async move {
            let cached = cached(&readers, &agent).await?;
            let mode_tier = mode.and_then(|mode| tier_of(cached.as_ref(), &mode));
            Ok(Facts {
                mode_tier,
                running_turns,
                ..Facts::default()
            })
        })
    }
}
