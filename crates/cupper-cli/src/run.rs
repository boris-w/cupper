//! The commands of design 5.2.14 on one connection: what each asks the host,
//! what it prints (text for people, or one JSON object per line with
//! `--json`), and how the process exits.
//!
//! Exit codes: 0 done; 1 the host refused (the error's semantic code is
//! printed); 2 a usage mistake; 3 the host is not running or has exited;
//! 4 the host could not be verified. `elevate`: 10 denied, 11 no decision in
//! time, 12 rate limited, 13 refused as an agent's descendant.

use std::collections::BTreeMap;
use std::path::PathBuf;

use cupper_protocol::error::{ErrorCode, ErrorReason, RpcError};
use cupper_protocol::ids::{RequestId, SessionId};
use cupper_protocol::interaction::{
    AnswerValue, FieldType, InteractionOption, InteractionRequest, InteractionRequested, OptionKind,
};
use cupper_protocol::methods::Empty;
use cupper_protocol::methods::admin::AuthElevate;
#[cfg(feature = "test-harness")]
use cupper_protocol::methods::admin::TestClockAdvance;
use cupper_protocol::methods::handshake::HostStatus;
use cupper_protocol::methods::interaction::InteractionRespond;
use cupper_protocol::methods::project::ProjectAdd;
use cupper_protocol::methods::session::{SessionCreate, SessionInterrupt, SessionSend};
use cupper_protocol::methods::stream::{HistoryPage, StreamUnsubscribe};
use cupper_protocol::rpc::{Method, Scope};
#[cfg(feature = "test-harness")]
use cupper_protocol::view::IndexView;
use cupper_protocol::view::{SessionView, View};
use cupper_sys::RefusalKind;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::args::{self, Command, Global, USAGE, UsageError};
use crate::build_kind::{BUILD, socket_path};
use crate::client::{Client, ClientAuth, ClientError, CloseReason, Incoming, Options, command_id};
use crate::mirror::{Mirror, fold, subscribe};
use crate::verify::HostCheck;

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("{0}\n\n{USAGE}")]
    Usage(#[from] UsageError),
    #[error("the host is not running, or has exited")]
    HostGone,
    #[error("the host could not be verified: {0}")]
    Unverified(String),
    #[error("{}", .0.message)]
    Refused(RpcError),
    #[error("{0}")]
    Other(String),
    #[error("the user did not grant the scopes")]
    Denied,
}

impl From<ClientError> for Failure {
    fn from(err: ClientError) -> Self {
        // A host that writes and closes is refused as gone before its data
        // is read (T049): the same as the end of the stream.
        let gone = |reason: &Option<CloseReason>| match reason {
            None | Some(CloseReason::Eof) => true,
            Some(CloseReason::Refused(refusal)) => refusal.kind == RefusalKind::PeerExited,
            _ => false,
        };
        match err {
            ClientError::Closed(reason) if gone(&reason) => Self::HostGone,
            ClientError::Io(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                Self::HostGone
            }
            ClientError::Verify(err) => Self::Unverified(err.to_string()),
            ClientError::Rpc(err) => Self::Refused(err),
            err => Self::Other(err.to_string()),
        }
    }
}

impl Failure {
    pub fn exit_code(&self) -> u8 {
        let refused = |err: &RpcError| match (err.app_code(), err.reason()) {
            (Some(ErrorCode::Timeout), _) => 11,
            (Some(ErrorCode::RateLimited), _) => 12,
            (_, Some(ErrorReason::Lineage)) => 13,
            _ => 1,
        };
        match self {
            Self::Usage(_) => 2,
            Self::HostGone => 3,
            Self::Unverified(_) => 4,
            Self::Refused(err) => refused(err),
            Self::Other(_) => 1,
            Self::Denied => 10,
        }
    }

    /// `{"error": {"code", "reason"?, "message", ...}}`, the semantic code of
    /// a refusal or this tool's own.
    fn json(&self) -> Value {
        let error = match self {
            Self::Refused(err) => match &err.data {
                Some(data) => json!(data).as_object().cloned().unwrap_or_default(),
                None => [("code".to_owned(), json!(err.code))].into_iter().collect(),
            },
            other => {
                let code = match other {
                    Self::Usage(_) => "usage",
                    Self::HostGone => "host_gone",
                    Self::Unverified(_) => "host_unverified",
                    Self::Denied => "denied",
                    _ => "failed",
                };
                [("code".to_owned(), json!(code))].into_iter().collect()
            }
        };
        let mut error = error;
        error.insert("message".to_owned(), json!(self.to_string().lines().next()));
        json!({ "error": error })
    }
}

/// Text for people, or JSON Lines.
#[derive(Debug, Clone, Copy)]
struct Out {
    json: bool,
}

impl Out {
    fn emit(&self, json: Value, text: impl FnOnce() -> String) {
        match self.json {
            true => println!("{json}"),
            false => println!("{}", text()),
        }
    }

    fn fail(&self, failure: &Failure) {
        match self.json {
            true => println!("{}", failure.json()),
            false => eprintln!("cupper: {failure}"),
        }
    }
}

/// Runs `command` as `global` says; the exit code.
pub async fn main(global: Global, command: Command) -> u8 {
    let out = Out { json: global.json };
    let result = match connect(&global).await {
        Ok(client) => Cli { client, out }.run(command).await,
        Err(failure) => Err(failure),
    };
    result.map_or_else(
        |failure| {
            out.fail(&failure);
            failure.exit_code()
        },
        |()| 0,
    )
}

async fn connect(global: &Global) -> Result<Client, Failure> {
    let check = HostCheck::of(BUILD).map_err(|err| Failure::Unverified(err.to_string()))?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let data_dir = global
        .data_dir
        .clone()
        .or_else(|| BUILD.default_data_dir(&home));
    let data_dir = data_dir.ok_or_else(|| UsageError("`--data-dir` is needed".into()))?;
    let socket = socket_path(&data_dir).map_err(|err| Failure::Other(err.to_string()))?;
    let auth = match global.harness_key_fd {
        #[cfg(feature = "test-harness")]
        Some(fd) => {
            let key =
                crate::harness::HarnessKey::read_fd(fd).map_err(|e| UsageError(e.to_string()))?;
            ClientAuth::Harness(key)
        }
        _ => ClientAuth::Peer,
    };
    if !global.json && check.is_weak(&auth) {
        eprintln!("cupper: the host is not verified by its signature");
    }
    let options = Options {
        auth,
        ..Options::default()
    };
    Ok(Client::connect(&socket, &check, options).await?)
}

struct Cli {
    client: Client,
    out: Out,
}

impl Cli {
    async fn run(&mut self, command: Command) -> Result<(), Failure> {
        let out = self.out;
        match command {
            Command::Status => {
                let status = self.call::<HostStatus>(json!(Empty {})).await?;
                out.emit(json!(status), || {
                    let (turns, pending) = (status.running_turns, status.pending_interactions);
                    format!(
                        "host {}: {turns} turns running, {pending} requests pending",
                        status.version
                    )
                });
            }
            Command::Sessions { watch } => self.sessions(watch).await?,
            Command::Watch(session) => self.watch(session).await?,
            Command::History(session) => self.history(session).await?,
            Command::New {
                agent,
                project,
                message,
            } => {
                let path = project.display().to_string();
                let added =
                    self.call::<ProjectAdd>(json!({"command_id": command_id(), "path": path}));
                let project_id = added.await?.project_id;
                let create = json!({"command_id": command_id(), "agent": agent, "project_id": project_id,
                    "workspace": {"kind": "local"}, "message": text(&message)});
                let session_id = self.call::<SessionCreate>(create).await?.session_id;
                let created = json!({"project_id": project_id, "session_id": session_id});
                out.emit(created, || session_id.to_string());
            }
            Command::Send {
                session,
                text: message,
                wait,
            } => {
                let send = json!({"command_id": command_id(), "session_id": session, "mode": "queue",
                    "content": text(&message)});
                let sent = self.call::<SessionSend>(send).await?;
                out.emit(json!(sent), || format!("{}", json!(sent.disposition)));
                if wait {
                    self.wait(session, sent.turn_id, sent.queue_id).await?;
                }
            }
            Command::Interrupt(session) => {
                let interrupt = json!({"command_id": command_id(), "session_id": session});
                let interrupted = self.call::<SessionInterrupt>(interrupt).await?;
                let turn = interrupted.turn_id.as_ref().map(ToString::to_string);
                out.emit(json!(interrupted), || {
                    turn.unwrap_or_else(|| "no turn ran".into())
                });
            }
            Command::Pending(session) => self.pending(session).await?,
            #[cfg(feature = "test-harness")]
            Command::ClockAdvance(ms) => {
                self.call::<TestClockAdvance>(json!({"ms": ms})).await?;
                out.emit(json!({"advanced_ms": ms}), || format!("advanced {ms} ms"));
            }
            #[cfg(feature = "test-harness")]
            Command::View(None) => {
                self.follow(Mirror::index(), |v: &IndexView| json!(v))
                    .await?
            }
            #[cfg(feature = "test-harness")]
            Command::View(Some(session)) => {
                let json = |v: &SessionView| json!(v);
                self.follow(Mirror::session(session), json).await?;
            }
            Command::Respond {
                session,
                request,
                approve,
                option,
            } => {
                self.respond(session, request, |asked| choose(asked, approve, option))
                    .await?;
            }
            Command::Answer {
                session,
                request,
                fields,
            } => {
                self.respond(session, request, |asked| answer(asked, &fields))
                    .await?;
            }
            Command::Elevate { scopes, reason } => self.elevate(scopes, reason).await?,
            Command::BuildInfo => println!("{}", crate::build_kind::build_info(BUILD)),
            Command::Repl => self.repl().await?,
        }
        Ok(())
    }

    async fn call<M: Method>(&self, params: Value) -> Result<M::Result, Failure> {
        let params = serde_json::from_value(params).map_err(|e| Failure::Other(e.to_string()))?;
        Ok(self.client.request::<M>(params).await?)
    }

    async fn next(&mut self) -> Result<Incoming, Failure> {
        let next = self.client.next().await;
        next.ok_or_else(|| ClientError::Closed(self.client.close_reason()).into())
    }

    /// Prints `{stream, seq, resubscribed, view}` each time `mirror` is live
    /// after a batch; `resubscribed` counts the times it subscribed again
    /// after a gap or a view it could not apply (design 5.3.3).
    #[cfg(feature = "test-harness")]
    async fn follow<V: View>(
        &mut self,
        mirror: Mirror<V>,
        json: fn(&V) -> Value,
    ) -> Result<(), Failure> {
        let mut mirror = self.live(mirror).await?;
        let mut resubscribed = 0;
        loop {
            let seq = mirror.resume().map(|resume| resume.after_seq);
            let line = json!({"stream": mirror.stream(), "seq": seq,
                "resubscribed": resubscribed, "view": json(mirror.view())});
            self.out
                .emit(line, || format!("{} at {seq:?}", mirror.stream()));
            loop {
                let incoming = self.next().await?;
                if let Some(params) = mirror.handle(&incoming).map_err(Failure::Refused)? {
                    resubscribed += 1;
                    self.client.subscribe(params).await?;
                }
                if mirror.is_live()
                    && matches!(incoming, Incoming::Events(_) | Incoming::Snapshot(_))
                {
                    break;
                }
            }
        }
    }

    /// `mirror`, subscribed and caught up.
    async fn live<V: View>(&mut self, mut mirror: Mirror<V>) -> Result<Mirror<V>, Failure> {
        subscribe(&self.client, &mut mirror).await?;
        while !mirror.is_live() {
            let incoming = self.next().await?;
            fold(&self.client, &mut mirror, &incoming).await?;
        }
        Ok(mirror)
    }

    /// A session's view as it is now; the subscription ends.
    async fn session(&mut self, session: &SessionId) -> Result<SessionView, Failure> {
        let mirror = self.live(Mirror::session(session.clone())).await?;
        let stream = mirror.stream().clone();
        self.call::<StreamUnsubscribe>(json!({ "stream": stream }))
            .await?;
        Ok(mirror.view().clone())
    }

    async fn sessions(&mut self, watch: bool) -> Result<(), Failure> {
        let mut index = self.live(Mirror::index()).await?;
        let mut shown = BTreeMap::new();
        loop {
            let now = &index.view().sessions;
            for (id, summary) in now
                .iter()
                .filter(|(id, summary)| shown.get(*id) != Some(*summary))
            {
                let title = format!("{id} {} {}", json!(summary.status), summary.title);
                self.out.emit(json!(summary), || title);
            }
            for gone in shown.keys().filter(|id| !now.contains_key(*id)) {
                self.out
                    .emit(json!({"session_id": gone, "removed": true}), || {
                        format!("{gone} removed")
                    });
            }
            shown = now.clone();
            if !watch {
                return Ok(());
            }
            loop {
                let incoming = self.next().await?;
                fold(&self.client, &mut index, &incoming).await?;
                if index.is_live()
                    && matches!(incoming, Incoming::Events(_) | Incoming::Snapshot(_))
                {
                    break;
                }
            }
        }
    }

    /// The session's events as they come, until the host goes.
    async fn watch(&mut self, session: SessionId) -> Result<(), Failure> {
        let mut mirror = self.live(Mirror::session(session)).await?;
        loop {
            let incoming = self.next().await?;
            if let Incoming::Events(events) = &incoming
                && events.stream == *mirror.stream()
            {
                for event in &events.events {
                    let event = json!(event);
                    self.out.emit(event.clone(), || line(&event));
                }
            }
            fold(&self.client, &mut mirror, &incoming).await?;
        }
    }

    async fn history(&mut self, session: SessionId) -> Result<(), Failure> {
        let mut cursor = None;
        loop {
            let ask = json!({"session_id": session, "cursor": cursor});
            let page = self.call::<HistoryPage>(ask).await?;
            for op in &page.ops {
                let op = json!(op);
                self.out.emit(op.clone(), || line(&op));
            }
            match page.next {
                Some(next) => cursor = Some(next),
                None => return Ok(()),
            }
        }
    }

    /// Until the turn of a message sent ends: `turn` when it started, or
    /// once queue entry `queued` went and no turn runs.
    async fn wait(
        &mut self,
        session: SessionId,
        turn: Option<cupper_protocol::ids::TurnId>,
        queued: Option<cupper_protocol::ids::QueueId>,
    ) -> Result<(), Failure> {
        let mut mirror = self.live(Mirror::session(session)).await?;
        loop {
            let view = mirror.view();
            let waiting =
                |id| (view.queue.iter()).any(|q| q.entries.iter().any(|e| Some(&e.queue_id) == id));
            let ended = match &turn {
                Some(turn) => view.turns.iter().find(|t| t.started.turn_id == *turn),
                None if !waiting(queued.as_ref()) => view.turns.last(),
                None => None,
            };
            if let Some(completed) = ended.and_then(|t| t.completed.as_ref()) {
                let status = json!(completed.status);
                self.out.emit(json!(completed), || {
                    format!("turn {} {status}", completed.turn_id)
                });
                return Ok(());
            }
            let incoming = self.next().await?;
            fold(&self.client, &mut mirror, &incoming).await?;
        }
    }

    async fn pending(&mut self, session: Option<SessionId>) -> Result<(), Failure> {
        let sessions = match session {
            Some(session) => vec![session],
            None => {
                let index = self.live(Mirror::index()).await?;
                let sessions = index.view().sessions.values();
                sessions
                    .filter(|s| s.pending_interactions > 0)
                    .map(|s| s.session_id.clone())
                    .collect()
            }
        };
        for session in sessions {
            let view = self.session(&session).await?;
            for request in view.requests.values().filter(|r| r.is_pending()) {
                let asked = &request.requested;
                let mut row = json!(asked);
                row["session_id"] = json!(session);
                self.out.emit(row, || {
                    format!("{session} {} {}", asked.request_id, summary(&asked.request))
                });
            }
        }
        Ok(())
    }

    /// Answers a pending request with what `outcome` makes of it.
    async fn respond(
        &mut self,
        session: SessionId,
        request: RequestId,
        outcome: impl FnOnce(&InteractionRequested) -> Result<Value, Failure>,
    ) -> Result<(), Failure> {
        let view = self.session(&session).await?;
        let found = view.requests.get(&request).filter(|r| r.is_pending());
        let asked = found.ok_or_else(|| Failure::Other(format!("no pending request {request}")))?;
        let outcome = outcome(&asked.requested)?;
        let respond = json!({"command_id": command_id(), "session_id": session, "request_id": request,
            "request_digest": asked.requested.digest, "outcome": outcome});
        let answered = self.call::<InteractionRespond>(respond).await?;
        let shown = json!({"request_id": request, "outcome": outcome, "result": answered});
        self.out.emit(shown, || format!("{request} answered"));
        Ok(())
    }

    async fn elevate(&mut self, scopes: Vec<Scope>, reason: Option<String>) -> Result<(), Failure> {
        let asked = self
            .call::<AuthElevate>(json!({"scopes": scopes, "reason": reason}))
            .await;
        let granted = asked?;
        let scopes = json!(granted.granted);
        self.out.emit(json!(granted), || match granted.expires_at {
            Some(until) => format!("granted {scopes} until {}", until.0),
            None => "denied".into(),
        });
        match granted.granted.is_empty() {
            true => Err(Failure::Denied),
            false => Ok(()),
        }
    }

    /// Commands from stdin, one per line, on this connection. With `--json`,
    /// each command's output ends with `{"repl": {"exit": <code>}}`.
    async fn repl(&mut self) -> Result<(), Failure> {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        let read = |err: std::io::Error| Failure::Other(err.to_string());
        while let Some(line) = lines.next_line().await.map_err(read)? {
            let words = args::split(&line);
            if words.as_ref().is_ok_and(|words| words.is_empty()) {
                continue;
            }
            if words
                .as_ref()
                .is_ok_and(|words| *words == ["exit"] || *words == ["quit"])
            {
                break;
            }
            let result = match words.and_then(|words| args::command(&words)) {
                Ok(Command::Repl | Command::BuildInfo) => {
                    Err(UsageError("not in `repl`".into()).into())
                }
                Ok(command) => Box::pin(self.run(command)).await,
                Err(err) => Err(err.into()),
            };
            let exit = result.as_ref().map_or_else(Failure::exit_code, |()| 0);
            match result {
                Err(Failure::HostGone) => return Err(Failure::HostGone),
                Err(failure) => self.out.fail(&failure),
                Ok(()) => {}
            }
            if self.out.json {
                println!("{}", json!({"repl": {"exit": exit}}));
            }
        }
        Ok(())
    }
}

/// A prompt's content: one text block.
fn text(message: &str) -> Value {
    json!([{"type": "text", "text": message}])
}

/// An event or history op for people: its seq and type.
fn line(event: &Value) -> String {
    let seq = event["seq"]
        .as_u64()
        .map(|seq| format!("{seq} "))
        .unwrap_or_default();
    format!("{seq}{}", event["type"].as_str().unwrap_or("?"))
}

fn summary(request: &InteractionRequest) -> String {
    match request {
        InteractionRequest::Permission(permission) => format!("permission: {}", permission.title),
        InteractionRequest::Question(question) => format!("question: {}", question.prompt),
        InteractionRequest::PlanConfirm(_) => "plan".into(),
        other => format!("{}", json!(other)["type"]),
    }
}

/// `approve` or `reject`: `option` when given, else the first option of the
/// kind, once before always; rejecting what has no such option dismisses it.
fn choose(
    asked: &InteractionRequested,
    approve: bool,
    option: Option<String>,
) -> Result<Value, Failure> {
    let options: &[InteractionOption] = match &asked.request {
        InteractionRequest::Permission(permission) => &permission.options,
        InteractionRequest::PlanConfirm(plan) => &plan.options,
        _ => &[],
    };
    let kinds = match approve {
        true => [OptionKind::AllowOnce, OptionKind::AllowAlways],
        false => [OptionKind::RejectOnce, OptionKind::RejectAlways],
    };
    let first = kinds
        .iter()
        .find_map(|kind| options.iter().find(|o| o.kind == *kind));
    match (option, first) {
        (Some(option_id), _) => Ok(json!({"type": "selected", "option_id": option_id})),
        (None, Some(option)) => Ok(json!({"type": "selected", "option_id": option.option_id})),
        (None, None) if !approve => Ok(json!({"type": "dismissed"})),
        (None, None) => Err(Failure::Other("nothing to approve: use `answer`".into())),
    }
}

/// `--field id=value` for each field of a question, by its type.
fn answer(asked: &InteractionRequested, fields: &[(String, String)]) -> Result<Value, Failure> {
    let InteractionRequest::Question(question) = &asked.request else {
        return Err(Failure::Other(
            "not a question: use `approve` or `reject`".into(),
        ));
    };
    let mut values = BTreeMap::new();
    for (id, value) in fields {
        let field = question.fields.iter().find(|field| field.field_id == *id);
        let field = field.ok_or_else(|| UsageError(format!("the question has no field `{id}`")))?;
        let bad = || UsageError(format!("`{id}` is not a {}", json!(field.field_type)));
        let value = match field.field_type {
            FieldType::MultiChoice => {
                AnswerValue::Choices(value.split(',').map(str::to_owned).collect())
            }
            FieldType::Boolean => AnswerValue::Boolean(value.parse().map_err(|_| bad())?),
            FieldType::Number => AnswerValue::Number(value.parse().map_err(|_| bad())?),
            _ => AnswerValue::Text(value.clone()),
        };
        values.insert(id.clone(), value);
    }
    Ok(json!({"type": "answer", "values": values}))
}
