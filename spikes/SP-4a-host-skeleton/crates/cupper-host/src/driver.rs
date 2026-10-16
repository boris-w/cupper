//! ACP driver: spawns one adapter process and talks ACP to it with the official
//! Rust SDK. ACP types stay inside this module; the rest of the host only sees
//! [`DriverEvent`] and the driver methods.
//!
//! The SDK runs our notification/request callbacks inside its dispatch loop, so
//! every callback only does an unbounded channel send and returns. Nothing on the
//! ACP read path waits for clients.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ClientCapabilities, ContentBlock, CreateElicitationRequest, CreateElicitationResponse,
    ElicitationAcceptAction, ElicitationAction, ElicitationCapabilities, ElicitationContentValue,
    ElicitationFormCapabilities, ElicitationMode, ElicitationPropertySchema, Implementation, InitializeRequest,
    NewSessionRequest, PermissionOptionKind as AcpOptionKind, PromptRequest, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, ResumeSessionRequest, SelectedPermissionOutcome,
    SessionConfigId, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOptions,
    SessionConfigValueId, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, TextContent, ToolCallContent, ToolCallStatus,
};
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectionTo};
use cupper_protocol::PermissionOptionKind;
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::sys;

/// How to start an agent adapter (from the host's agents config).
#[derive(Debug, Clone, Deserialize)]
pub struct AgentSpec {
    pub command: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment for this adapter, on top of the allow-list.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Preferred model: the first model option whose value contains this text.
    #[serde(default)]
    pub model: Option<String>,
    /// Permission mode to select after `session/new` (exact option value).
    /// Sessions fail to start if the agent does not offer it.
    #[serde(default)]
    pub mode: Option<String>,
}

/// Host environment variables an adapter may inherit. Everything else,
/// including every `CUPPER_*` variable, is dropped.
const ENV_ALLOW: &[&str] = &["PATH", "HOME", "USER", "LOGNAME", "SHELL", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE", "TERM"];

fn adapter_env(spec: &AgentSpec) -> Vec<(String, String)> {
    let mut env: BTreeMap<String, String> =
        std::env::vars().filter(|(k, _)| ENV_ALLOW.contains(&k.as_str())).collect();
    for (k, v) in &spec.env {
        if !k.starts_with("CUPPER_") {
            env.insert(k.clone(), v.clone());
        }
    }
    env.into_iter().collect()
}

/// Agent output, normalized into cupper's own terms.
#[derive(Debug, Clone, PartialEq)]
pub enum NormUpdate {
    AgentText { message_id: Option<String>, text: String },
    Thought { text: String },
    ToolCall { id: String, title: String, kind: String, status: Option<ToolState>, content: Option<String> },
    ToolUpdate { id: String, title: Option<String>, status: Option<ToolState>, content: Option<String> },
    Plan { text: String },
    Other { kind: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PermissionAsk {
    pub tool_call_id: String,
    pub title: String,
    pub options: Vec<(String, String, PermissionOptionKind)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionAnswer {
    Selected(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestionAsk {
    pub prompt: String,
    /// Form field that receives the answer (single-field forms only).
    pub field: Option<String>,
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionAnswer {
    Text(String),
    Declined,
}

#[derive(Debug)]
pub enum DriverEvent {
    Update(NormUpdate),
    Permission(PermissionAsk, oneshot::Sender<PermissionAnswer>),
    Question(QuestionAsk, oneshot::Sender<QuestionAnswer>),
    PromptDone { stop_reason: String },
    PromptFailed { message: String },
    /// The adapter process exited (pid).
    Exited(i32),
}

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("spawn failed: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("agent error: {0}")]
    Acp(String),
    #[error("adapter did not answer in time")]
    Timeout,
    #[error("adapter connection closed")]
    Closed,
}

impl From<agent_client_protocol::Error> for DriverError {
    fn from(e: agent_client_protocol::Error) -> Self {
        DriverError::Acp(e.to_string())
    }
}

enum Cmd {
    NewSession { cwd: PathBuf, prefs: Prefs, reply: oneshot::Sender<Result<String, DriverError>> },
    Resume { session: String, cwd: PathBuf, prefs: Prefs, reply: oneshot::Sender<Result<(), DriverError>> },
    Prompt { session: String, text: String },
    Cancel { session: String },
}

/// Session settings applied right after `session/new`.
#[derive(Debug, Clone, Default)]
struct Prefs {
    model: Option<String>,
    mode: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct AgentCaps {
    pub resume: bool,
}

/// A running adapter.
#[derive(Debug)]
pub struct Driver {
    cmd_tx: mpsc::UnboundedSender<Cmd>,
    pub pid: i32,
    pub start_us: u64,
    pub caps: AgentCaps,
    prefs: Prefs,
}

const START_TIMEOUT: Duration = Duration::from_secs(60);

impl Driver {
    /// Spawn the adapter and complete `initialize`.
    pub async fn start(spec: &AgentSpec, log_path: &Path, events: mpsc::UnboundedSender<DriverEvent>) -> Result<Self, DriverError> {
        let program = resolve_program(&spec.command)?;
        let spawned = sys::spawn_adapter(&program, &spec.args, &adapter_env(spec), log_path)?;
        let pid = spawned.pid;
        let reaper_events = events.clone();
        tokio::task::spawn_blocking(move || {
            let status = sys::wait_blocking(pid);
            tracing::info!(pid, ?status, "adapter exited");
            let _ = reaper_events.send(DriverEvent::Exited(pid));
        });
        let stdin = tokio::net::unix::pipe::Sender::from_owned_fd(spawned.stdin)?;
        let stdout = tokio::net::unix::pipe::Receiver::from_owned_fd(spawned.stdout)?;
        let transport = ByteStreams::new(stdin.compat_write(), stdout.compat());
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = oneshot::channel();
        tokio::spawn(run_connection(transport, events, cmd_rx, ready_tx));
        let caps = match tokio::time::timeout(START_TIMEOUT, ready_rx).await {
            Ok(Ok(caps)) => caps,
            Ok(Err(_)) => {
                kill(pid);
                return Err(DriverError::Closed);
            }
            Err(_) => {
                kill(pid);
                return Err(DriverError::Timeout);
            }
        };
        let prefs = Prefs { model: spec.model.clone(), mode: spec.mode.clone() };
        Ok(Driver { cmd_tx, pid, start_us: spawned.start_us, caps, prefs })
    }

    async fn call<T>(&self, make: impl FnOnce(oneshot::Sender<Result<T, DriverError>>) -> Cmd) -> Result<T, DriverError> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx.send(make(tx)).map_err(|_| DriverError::Closed)?;
        tokio::time::timeout(START_TIMEOUT, rx).await.map_err(|_| DriverError::Timeout)?.map_err(|_| DriverError::Closed)?
    }

    pub async fn new_session(&self, cwd: &Path) -> Result<String, DriverError> {
        let prefs = self.prefs.clone();
        self.call(|reply| Cmd::NewSession { cwd: cwd.to_path_buf(), prefs, reply }).await
    }

    pub async fn resume(&self, session: &str, cwd: &Path) -> Result<(), DriverError> {
        let prefs = self.prefs.clone();
        self.call(|reply| Cmd::Resume { session: session.to_owned(), cwd: cwd.to_path_buf(), prefs, reply }).await
    }

    /// Start a turn; completion arrives as [`DriverEvent::PromptDone`].
    pub fn prompt(&self, session: &str, text: &str) -> Result<(), DriverError> {
        self.cmd_tx.send(Cmd::Prompt { session: session.to_owned(), text: text.to_owned() }).map_err(|_| DriverError::Closed)
    }

    pub fn cancel(&self, session: &str) -> Result<(), DriverError> {
        self.cmd_tx.send(Cmd::Cancel { session: session.to_owned() }).map_err(|_| DriverError::Closed)
    }

    /// Terminate the adapter's whole process group.
    pub fn shutdown(&self) {
        kill(self.pid);
    }
}

fn kill(pgid: i32) {
    let _ = sys::kill_group(pgid, libc::SIGTERM);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let _ = sys::kill_group(pgid, libc::SIGKILL);
    });
}

fn resolve_program(cmd: &Path) -> std::io::Result<PathBuf> {
    if cmd.components().count() > 1 {
        return Ok(cmd.to_path_buf());
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|dir| dir.join(cmd))
        .find(|p| p.is_file())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} not found in PATH", cmd.display())))
}

async fn run_connection<O, I>(
    transport: ByteStreams<O, I>,
    events: mpsc::UnboundedSender<DriverEvent>,
    mut cmd_rx: mpsc::UnboundedReceiver<Cmd>,
    ready_tx: oneshot::Sender<AgentCaps>,
) where
    O: futures::AsyncWrite + Send + 'static,
    I: futures::AsyncRead + Send + 'static,
{
    let (ev_update, ev_perm, ev_question) = (events.clone(), events.clone(), events.clone());
    let result = Client
        .builder()
        .name("cupper-host")
        .on_receive_notification(
            async move |n: SessionNotification, _cx| {
                let _ = ev_update.send(DriverEvent::Update(normalize(n.update)));
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |req: RequestPermissionRequest, responder, _cx| {
                let (tx, rx) = oneshot::channel();
                let _ = ev_perm.send(DriverEvent::Permission(permission_ask(&req), tx));
                tokio::spawn(async move {
                    let outcome = match rx.await {
                        Ok(PermissionAnswer::Selected(id)) => RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)),
                        _ => RequestPermissionOutcome::Cancelled,
                    };
                    let _ = responder.respond(RequestPermissionResponse::new(outcome));
                });
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: CreateElicitationRequest, responder, _cx| {
                let ask = question_ask(&req);
                let field = ask.field.clone();
                let (tx, rx) = oneshot::channel();
                let _ = ev_question.send(DriverEvent::Question(ask, tx));
                tokio::spawn(async move {
                    let action = match (rx.await, field) {
                        (Ok(QuestionAnswer::Text(t)), Some(f)) => ElicitationAction::Accept(
                            ElicitationAcceptAction::new().content(BTreeMap::from([(f, ElicitationContentValue::from(t))])),
                        ),
                        (Ok(QuestionAnswer::Declined), _) => ElicitationAction::Decline,
                        _ => ElicitationAction::Cancel,
                    };
                    let _ = responder.respond(CreateElicitationResponse::new(action));
                });
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(transport, async move |cx: ConnectionTo<Agent>| {
            let caps = ClientCapabilities::new()
                .elicitation(ElicitationCapabilities::new().form(ElicitationFormCapabilities::new()));
            let init = cx
                .send_request(
                    InitializeRequest::new(ProtocolVersion::V1)
                        .client_capabilities(caps)
                        .client_info(Implementation::new("cupper-host", env!("CARGO_PKG_VERSION"))),
                )
                .block_task()
                .await?;
            let _ = ready_tx.send(AgentCaps { resume: init.agent_capabilities.session_capabilities.resume.is_some() });
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    Cmd::NewSession { cwd, prefs, reply } => {
                        let r = new_session(&cx, cwd, &prefs).await;
                        let _ = reply.send(r);
                    }
                    Cmd::Resume { session, cwd, prefs, reply } => {
                        let r = match cx.send_request(ResumeSessionRequest::new(session.clone(), cwd)).block_task().await {
                            Ok(resp) => apply_prefs(&cx, &session, resp.config_options.as_ref(), &prefs).await,
                            Err(e) => Err(e.into()),
                        };
                        let _ = reply.send(r);
                    }
                    Cmd::Prompt { session, text } => {
                        let (cx2, ev) = (cx.clone(), events.clone());
                        cx.spawn(async move {
                            let req = PromptRequest::new(session, vec![ContentBlock::Text(TextContent::new(text))]);
                            let msg = match cx2.send_request(req).block_task().await {
                                Ok(r) => DriverEvent::PromptDone { stop_reason: wire_name(&r.stop_reason) },
                                Err(e) => DriverEvent::PromptFailed { message: e.to_string() },
                            };
                            let _ = ev.send(msg);
                            Ok(())
                        })?;
                    }
                    Cmd::Cancel { session } => cx.send_notification(CancelNotification::new(session))?,
                }
            }
            Ok(())
        })
        .await;
    if let Err(e) = result {
        tracing::warn!("ACP connection ended: {e}");
    }
}

/// Find a select option of `category` whose value satisfies `pick`.
fn find_option(
    options: Option<&Vec<SessionConfigOption>>,
    category: SessionConfigOptionCategory,
    pick: impl Fn(&str) -> bool,
) -> Option<(SessionConfigId, SessionConfigValueId)> {
    options.into_iter().flatten().find_map(|o| {
        let SessionConfigKind::Select(sel) = &o.kind else { return None };
        if o.category.as_ref() != Some(&category) {
            return None;
        }
        let SessionConfigSelectOptions::Ungrouped(opts) = &sel.options else { return None };
        opts.iter().find(|v| pick(&v.value.to_string())).map(|v| (o.id.clone(), v.value.clone()))
    })
}

async fn new_session(cx: &ConnectionTo<Agent>, cwd: PathBuf, prefs: &Prefs) -> Result<String, DriverError> {
    let resp = cx.send_request(NewSessionRequest::new(cwd)).block_task().await?;
    let session = resp.session_id.to_string();
    apply_prefs(cx, &session, resp.config_options.as_ref(), prefs).await?;
    Ok(session)
}

/// Select the configured permission mode and model (also after a resume, where
/// the agent would otherwise restore whatever mode the transcript had).
async fn apply_prefs(cx: &ConnectionTo<Agent>, session: &str, options: Option<&Vec<SessionConfigOption>>, prefs: &Prefs) -> Result<(), DriverError> {
    if let Some(want) = &prefs.mode {
        let (id, value) = find_option(options, SessionConfigOptionCategory::Mode, |v| v == want)
            .ok_or_else(|| DriverError::Acp(format!("agent does not offer mode {want}")))?;
        cx.send_request(SetSessionConfigOptionRequest::new(session.to_owned(), id, value)).block_task().await?;
        tracing::info!(mode = %want, "selected permission mode");
    }
    if let Some(want) = &prefs.model {
        let want = want.to_lowercase();
        match find_option(options, SessionConfigOptionCategory::Model, |v| v.to_lowercase().contains(&want)) {
            Some((id, value)) => {
                cx.send_request(SetSessionConfigOptionRequest::new(session.to_owned(), id, value.clone())).block_task().await?;
                tracing::info!(model = %value, "selected model");
            }
            None => tracing::warn!(model = %want, "no matching model option"),
        }
    }
    Ok(())
}

/// snake_case wire name of a serde enum value.
fn wire_name<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_else(|| "unknown".into())
}

fn tool_state(s: ToolCallStatus) -> ToolState {
    match s {
        ToolCallStatus::Completed => ToolState::Completed,
        ToolCallStatus::Failed => ToolState::Failed,
        _ => ToolState::Running,
    }
}

fn render_content(items: &[ToolCallContent]) -> Option<String> {
    let parts: Vec<String> = items
        .iter()
        .map(|c| match c {
            ToolCallContent::Content(c) => match &c.content {
                ContentBlock::Text(t) => t.text.clone(),
                other => format!("[{}]", wire_name(other).as_str()),
            },
            ToolCallContent::Diff(d) => format!("diff {}\n{}", d.path.display(), d.new_text),
            ToolCallContent::Terminal(t) => format!("[terminal {}]", t.terminal_id),
            _ => "[unsupported content]".to_owned(),
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn normalize(update: SessionUpdate) -> NormUpdate {
    let text_of = |b: &ContentBlock| match b {
        ContentBlock::Text(t) => Some(t.text.clone()),
        _ => None,
    };
    match update {
        SessionUpdate::AgentMessageChunk(c) => match text_of(&c.content) {
            Some(text) => NormUpdate::AgentText { message_id: c.message_id.map(|m| m.to_string()), text },
            None => NormUpdate::Other { kind: "agent_message_non_text".into() },
        },
        SessionUpdate::AgentThoughtChunk(c) => match text_of(&c.content) {
            Some(text) => NormUpdate::Thought { text },
            None => NormUpdate::Other { kind: "agent_thought_non_text".into() },
        },
        SessionUpdate::ToolCall(tc) => NormUpdate::ToolCall {
            id: tc.tool_call_id.to_string(),
            title: tc.title,
            kind: wire_name(&tc.kind),
            status: Some(tool_state(tc.status)),
            content: render_content(&tc.content),
        },
        SessionUpdate::ToolCallUpdate(u) => NormUpdate::ToolUpdate {
            id: u.tool_call_id.to_string(),
            title: u.fields.title,
            status: u.fields.status.map(tool_state),
            content: u.fields.content.as_deref().and_then(render_content),
        },
        SessionUpdate::Plan(p) => NormUpdate::Plan {
            text: p.entries.iter().map(|e| format!("- [{}] {}", wire_name(&e.status), e.content)).collect::<Vec<_>>().join("\n"),
        },
        other => {
            let kind = serde_json::to_value(&other)
                .ok()
                .and_then(|v| v.get("sessionUpdate").and_then(|k| k.as_str()).map(str::to_owned))
                .unwrap_or_else(|| "unknown".into());
            NormUpdate::Other { kind }
        }
    }
}

fn permission_ask(req: &RequestPermissionRequest) -> PermissionAsk {
    let title = req.tool_call.fields.title.clone().unwrap_or_else(|| format!("tool {}", req.tool_call.tool_call_id));
    let options = req
        .options
        .iter()
        .map(|o| {
            let kind = match o.kind {
                AcpOptionKind::AllowOnce => PermissionOptionKind::AllowOnce,
                AcpOptionKind::AllowAlways => PermissionOptionKind::AllowAlways,
                AcpOptionKind::RejectOnce => PermissionOptionKind::RejectOnce,
                AcpOptionKind::RejectAlways => PermissionOptionKind::RejectAlways,
                ref other => PermissionOptionKind::Unknown(wire_name(other)),
            };
            (o.option_id.to_string(), o.name.clone(), kind)
        })
        .collect();
    PermissionAsk { tool_call_id: req.tool_call.tool_call_id.to_string(), title, options }
}

fn question_ask(req: &CreateElicitationRequest) -> QuestionAsk {
    let (field, choices) = match &req.mode {
        ElicitationMode::Form(form) if form.requested_schema.properties.len() == 1 => {
            let (name, schema) = form.requested_schema.properties.iter().next().expect("one property");
            let choices = match schema {
                ElicitationPropertySchema::String(s) => s.enum_values.clone().unwrap_or_default(),
                _ => Vec::new(),
            };
            (Some(name.clone()), choices)
        }
        _ => (None, Vec::new()),
    };
    QuestionAsk { prompt: req.message.clone(), field, choices }
}
