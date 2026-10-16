//! Scripted ACP agent for host tests.
//!
//! The prompt text is the script: commands separated by `;`. Anything that is
//! not a known command is answered with `fake reply to: <text>`.
//!
//! | command | effect |
//! |---|---|
//! | `say <text>` / `think <text>` | one message / thought chunk |
//! | `flood <n> <len>` | `n` message chunks of `len` chars, as fast as possible |
//! | `flood_mixed <n> <len>` | like `flood`, alternating message and thought chunks |
//! | `big <bytes>` | one message chunk of `bytes` chars |
//! | `tool <id> <kind> <title>` | start a tool call |
//! | `long_tool <id> <len>` | start a tool call whose title is `len` chars |
//! | `tool_done <id> <bytes>` | complete a tool call with `bytes` chars of output |
//! | `permission <tool_id>` | ask for permission, then `say permission:<option>` |
//! | `question <prompt>` | elicit a one-field form, then `say answer:<value>` |
//! | `sleep <ms>` / `wait_cancel` | wait (the latter until `session/cancel`) |
//! | `whoami` | `say session=<id> resumed=<bool>` |
//! | `probe` | `say fds=<fds open at process start> env=<variable names>` |
//! | `spawn_child` | start a long-lived grandchild in our process group |
//! | `exit <code>` | exit the process immediately |

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, CancelNotification, ContentBlock, ContentChunk, CreateElicitationRequest, ElicitationAction,
    ElicitationContentValue, ElicitationFormMode, ElicitationSchema, ElicitationSessionScope, InitializeRequest,
    InitializeResponse, NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionKind, PromptRequest,
    PromptResponse, RequestPermissionOutcome, RequestPermissionRequest, ResumeSessionRequest, ResumeSessionResponse,
    SessionCapabilities, SessionNotification, SessionResumeCapabilities, SessionUpdate, StopReason, TextContent,
    ToolCall, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Stdio};
use tokio::sync::watch;

#[derive(Default)]
struct SessionState {
    resumed: bool,
    cancel: Option<watch::Sender<bool>>,
}

type Sessions = Arc<Mutex<HashMap<String, SessionState>>>;

/// File descriptors open when the process started, i.e. the ones it inherited.
static INHERITED_FDS: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn inherited_fds() -> String {
    // Opening /dev/null takes the lowest free fd; reading /dev/fd reuses it.
    let lowest_free = std::fs::File::open("/dev/null").map(|f| std::os::fd::AsRawFd::as_raw_fd(&f)).unwrap_or(-1);
    let mut fds: Vec<i32> = std::fs::read_dir("/dev/fd")
        .map(|d| d.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok()).collect())
        .unwrap_or_default();
    fds.retain(|fd| *fd != lowest_free);
    fds.sort();
    fds.iter().map(i32::to_string).collect::<Vec<_>>().join(",")
}

fn main() -> anyhow::Result<()> {
    INHERITED_FDS.get_or_init(inherited_fds);
    tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(serve())
}

async fn serve() -> anyhow::Result<()> {
    let sessions: Sessions = Arc::default();
    let (s_new, s_resume, s_prompt, s_cancel) = (sessions.clone(), sessions.clone(), sessions.clone(), sessions);
    Agent
        .builder()
        .name("cupper-fake-agent")
        .on_receive_request(
            async move |req: InitializeRequest, responder, _cx| {
                let caps = AgentCapabilities::new()
                    .session_capabilities(SessionCapabilities::new().resume(SessionResumeCapabilities::new()));
                responder.respond(InitializeResponse::new(req.protocol_version).agent_capabilities(caps))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: NewSessionRequest, responder, _cx| {
                let id = format!("fake-{}-{}", std::process::id(), s_new.lock().unwrap().len() + 1);
                s_new.lock().unwrap().insert(id.clone(), SessionState::default());
                responder.respond(NewSessionResponse::new(id))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: ResumeSessionRequest, responder, _cx| {
                let id = req.session_id.to_string();
                s_resume.lock().unwrap().insert(id, SessionState { resumed: true, cancel: None });
                responder.respond(ResumeSessionResponse::new())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: PromptRequest, responder, cx| {
                let session_id = req.session_id.to_string();
                let (tx, rx) = watch::channel(false);
                let resumed = {
                    let mut map = s_prompt.lock().unwrap();
                    let st = map.entry(session_id.clone()).or_default();
                    st.cancel = Some(tx);
                    st.resumed
                };
                let text: String = req
                    .prompt
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text(t) => Some(t.text.as_str()),
                        _ => None,
                    })
                    .collect();
                let script = Script { cx: cx.clone(), session_id, resumed, cancel: rx };
                cx.spawn(async move {
                    let reason = script.run(&text).await?;
                    responder.respond(PromptResponse::new(reason))
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |n: CancelNotification, _cx| {
                if let Some(st) = s_cancel.lock().unwrap().get(n.session_id.to_string().as_str())
                    && let Some(tx) = &st.cancel
                {
                    let _ = tx.send(true);
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(Stdio::new())
        .await?;
    Ok(())
}

struct Script {
    cx: ConnectionTo<Client>,
    session_id: String,
    resumed: bool,
    cancel: watch::Receiver<bool>,
}

type R<T> = Result<T, agent_client_protocol::Error>;

impl Script {
    async fn run(mut self, text: &str) -> R<StopReason> {
        let commands: Vec<&str> = text.split(';').map(str::trim).filter(|c| !c.is_empty()).collect();
        let scripted = commands.iter().all(|c| is_command(c));
        if !scripted {
            self.say(&format!("fake reply to: {text}"))?;
            return Ok(StopReason::EndTurn);
        }
        for cmd in commands {
            if *self.cancel.borrow() {
                return Ok(StopReason::Cancelled);
            }
            let (name, rest) = cmd.split_once(' ').unwrap_or((cmd, ""));
            let args: Vec<&str> = rest.split_whitespace().collect();
            let num = |i: usize| args.get(i).and_then(|a| a.parse::<usize>().ok()).unwrap_or(0);
            match name {
                "say" => self.say(rest)?,
                "think" => self.update(SessionUpdate::AgentThoughtChunk(chunk(rest)))?,
                "flood" | "flood_mixed" => {
                    for i in 0..num(0) {
                        let body = filler(num(1), i);
                        if name == "flood_mixed" && i % 2 == 1 {
                            self.update(SessionUpdate::AgentThoughtChunk(chunk(&body)))?;
                        } else {
                            self.say(&body)?;
                        }
                    }
                }
                "big" => self.say(&filler(num(0), 0))?,
                "tool" => {
                    let id = args.first().copied().unwrap_or("tool");
                    let title = args.get(2..).map(|t| t.join(" ")).unwrap_or_default();
                    let call = ToolCall::new(id.to_owned(), title).kind(tool_kind(args.get(1).copied().unwrap_or("other")));
                    self.update(SessionUpdate::ToolCall(call))?;
                }
                "long_tool" => {
                    let id = args.first().copied().unwrap_or("tool");
                    self.update(SessionUpdate::ToolCall(ToolCall::new(id.to_owned(), filler(num(1), 23))))?;
                }
                "tool_done" => {
                    let id = args.first().copied().unwrap_or("tool");
                    let fields = ToolCallUpdateFields::new()
                        .status(ToolCallStatus::Completed)
                        .content(vec![ContentBlock::Text(TextContent::new(filler(num(1), 0))).into()]);
                    self.update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(id.to_owned(), fields)))?;
                }
                "permission" => self.permission(args.first().copied().unwrap_or("tool")).await?,
                "question" => self.question(rest).await?,
                "sleep" => {
                    if self.wait_or_cancel(Some(Duration::from_millis(num(0) as u64))).await {
                        return Ok(StopReason::Cancelled);
                    }
                }
                "wait_cancel" => {
                    self.wait_or_cancel(None).await;
                    return Ok(StopReason::Cancelled);
                }
                "whoami" => {
                    let line = format!("session={} resumed={}", self.session_id, self.resumed);
                    self.say(&line)?;
                }
                "probe" => {
                    let mut names: Vec<String> = std::env::vars().map(|(k, _)| k).collect();
                    names.sort();
                    let line = format!("fds={} env={}", INHERITED_FDS.get().map_or("?", String::as_str), names.join(","));
                    self.say(&line)?;
                }
                "spawn_child" => {
                    // Simulates a CLI child that outlives its adapter; the host must reap it by process group.
                    std::process::Command::new("/bin/sleep")
                        .arg("3600")
                        .spawn()
                        .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;
                }
                "exit" => std::process::exit(num(0) as i32),
                _ => unreachable!("filtered by is_command"),
            }
        }
        Ok(if *self.cancel.borrow() { StopReason::Cancelled } else { StopReason::EndTurn })
    }

    fn update(&self, update: SessionUpdate) -> R<()> {
        self.cx.send_notification(SessionNotification::new(self.session_id.clone(), update))
    }

    fn say(&self, text: &str) -> R<()> {
        self.update(SessionUpdate::AgentMessageChunk(chunk(text)))
    }

    /// Returns true if the turn was cancelled while waiting.
    async fn wait_or_cancel(&mut self, timeout: Option<Duration>) -> bool {
        let cancelled = self.cancel.wait_for(|c| *c);
        match timeout {
            Some(t) => tokio::time::timeout(t, cancelled).await.is_ok_and(|r| r.is_ok()),
            None => cancelled.await.is_ok(),
        }
    }

    async fn permission(&self, tool_id: &str) -> R<()> {
        let options = vec![
            PermissionOption::new("allow", "Allow once", PermissionOptionKind::AllowOnce),
            PermissionOption::new("always", "Always allow", PermissionOptionKind::AllowAlways),
            PermissionOption::new("reject", "Reject", PermissionOptionKind::RejectOnce),
        ];
        let call = ToolCallUpdate::new(tool_id.to_owned(), ToolCallUpdateFields::new().title(format!("Run {tool_id}")));
        let req = RequestPermissionRequest::new(self.session_id.clone(), call, options);
        let resp = self.cx.send_request(req).block_task().await?;
        let picked = match resp.outcome {
            RequestPermissionOutcome::Selected(s) => s.option_id.to_string(),
            _ => "cancelled".to_owned(),
        };
        self.say(&format!("permission:{picked}"))
    }

    async fn question(&self, prompt: &str) -> R<()> {
        let schema = ElicitationSchema::new().string("answer", true);
        let mode = ElicitationFormMode::new(ElicitationSessionScope::new(self.session_id.clone()), schema);
        let resp = self.cx.send_request(CreateElicitationRequest::new(mode, prompt)).block_task().await?;
        let answer = match resp.action {
            ElicitationAction::Accept(a) => match a.content.as_ref().and_then(|c: &BTreeMap<_, _>| c.get("answer")) {
                Some(ElicitationContentValue::String(s)) => s.clone(),
                _ => "empty".to_owned(),
            },
            _ => "declined".to_owned(),
        };
        self.say(&format!("answer:{answer}"))
    }
}

fn is_command(c: &str) -> bool {
    const NAMES: &[&str] = &[
        "say", "think", "flood", "flood_mixed", "big", "tool", "long_tool", "tool_done", "permission", "question", "sleep",
        "wait_cancel", "whoami", "probe", "spawn_child", "exit",
    ];
    NAMES.contains(&c.split_once(' ').map_or(c, |(n, _)| n))
}

fn chunk(text: &str) -> ContentChunk {
    ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
}

/// Deterministic text so tests can check content: `len` chars of one letter.
fn filler(len: usize, i: usize) -> String {
    let c = (b'a' + (i % 26) as u8) as char;
    std::iter::repeat_n(c, len).collect()
}

fn tool_kind(s: &str) -> ToolKind {
    match s {
        "read" => ToolKind::Read,
        "edit" => ToolKind::Edit,
        "execute" => ToolKind::Execute,
        "search" => ToolKind::Search,
        _ => ToolKind::Other,
    }
}
