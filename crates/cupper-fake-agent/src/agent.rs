//! The SDK's agent role: sessions, the global invariant (K1), steering, the
//! crash state, and the notifications each persona sends after a response.
//! Requests are handled untyped so that results keep the exact shapes of the
//! recordings; their params still go through the SDK's types, so the fake
//! rejects what a real adapter's SDK would.

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use agent_client_protocol::schema::v1::{
    CloseSessionRequest, ContentBlock, InitializeRequest, LoadSessionRequest, NewSessionRequest,
    PromptRequest, ResumeSessionRequest, SetSessionConfigOptionRequest,
};
use agent_client_protocol::{
    Agent, Client, ConnectTo, ConnectionTo, Error, JsonRpcMessage, Lines, Responder, UntypedMessage,
};
use futures::{Sink, Stream};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::watch;

use crate::config::{Config, Persona};
use crate::persona;
use crate::script::{self, Script};

/// Exit code when a second prompt arrives on a session while one is in flight.
pub const EXIT_SECOND_PROMPT: i32 = 70;

pub type State = Arc<Mutex<Shared>>;

pub struct Shared {
    pub config: Config,
    /// `clientInfo` of `initialize` (K26).
    pub client_info: Value,
    pub sessions: HashMap<String, Session>,
    created: u64,
    prompts: u64,
}

pub struct Session {
    pub cwd: PathBuf,
    pub resumed: bool,
    pub config: Vec<Value>,
    /// The prompt in flight: its number and its cancel signal.
    pub inflight: Option<(u64, watch::Sender<bool>)>,
    /// Claude's CLI died (`crash_cli`): requests fail until `session/resume`.
    pub crashed: bool,
    pub steer: Steer,
}

/// How `_session/steering` is answered: like the persona, or as a script set.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Steer {
    Persona,
    Inject,
    NewTurn,
    Hang,
}

pub fn lock(state: &State) -> MutexGuard<'_, Shared> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("the ACP connection failed")]
    Connection(#[source] Error),
}

/// Plays the agent on `transport` until the client goes away.
pub async fn serve(
    config: Config,
    transport: impl ConnectTo<Agent> + 'static,
) -> Result<(), ServeError> {
    let state = Arc::new(Mutex::new(Shared {
        config,
        client_info: Value::Null,
        sessions: HashMap::new(),
        created: 0,
        prompts: 0,
    }));
    let notified = state.clone();
    Agent
        .builder()
        .name("cupper-fake-agent")
        .on_receive_request(
            async move |req: UntypedMessage,
                        responder: Responder<Value>,
                        cx: ConnectionTo<Client>| {
                request(&state, req, responder, &cx)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |n: UntypedMessage, _cx: ConnectionTo<Client>| {
                notification(&notified, &n);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(transport)
        .await
        .map_err(ServeError::Connection)
}

type LineSink = Pin<Box<dyn Sink<String, Error = std::io::Error> + Send>>;
type LineStream = Pin<Box<dyn Stream<Item = std::io::Result<String>> + Send>>;

/// Newline-delimited JSON-RPC on stdin and stdout.
pub fn stdio() -> Lines<LineSink, LineStream> {
    let incoming = futures::stream::unfold(
        BufReader::new(tokio::io::stdin()).lines(),
        |mut lines| async move {
            lines
                .next_line()
                .await
                .transpose()
                .map(|line| (line, lines))
        },
    );
    let outgoing =
        futures::sink::unfold(tokio::io::stdout(), |mut stdout, line: String| async move {
            let mut bytes = line.into_bytes();
            bytes.push(b'\n');
            stdout.write_all(&bytes).await?;
            stdout.flush().await?;
            Ok::<_, std::io::Error>(stdout)
        });
    Lines::new(Box::pin(outgoing), Box::pin(incoming))
}

fn request(
    state: &State,
    req: UntypedMessage,
    responder: Responder<Value>,
    cx: &ConnectionTo<Client>,
) -> Result<(), Error> {
    let (method, params) = req.into_parts();
    match method.as_str() {
        "session/prompt" => return prompt(state, &params, responder, cx),
        "_session/steering" => return steering(state, &params, responder, cx),
        // The responder is dropped unanswered: the host's `initialize` times out.
        "initialize" if lock(state).config.fault("initialize_hang") => return Ok(()),
        _ => {}
    }
    let mut after = Vec::new();
    let result = answer(&mut lock(state), &method, &params, &mut after);
    responder.respond_with_result(result)?;
    for (method, params) in after {
        cx.send_notification(UntypedMessage::new(method, params)?)?;
    }
    Ok(())
}

/// The result of a request answered at once, and the notifications that follow it.
fn answer(
    shared: &mut Shared,
    method: &str,
    params: &Value,
    after: &mut Vec<(&'static str, Value)>,
) -> Result<Value, Error> {
    let persona = shared.config.persona;
    match method {
        "initialize" => {
            let req = InitializeRequest::parse_message(method, params)?;
            shared.client_info = serde_json::to_value(req.client_info).unwrap_or_default();
            after.push((
                "_auth/status_update",
                persona::auth_status(persona, shared.config.logged_in),
            ));
            Ok(persona::initialize(persona))
        }
        "session/new" => {
            NewSessionRequest::parse_message(method, params)?;
            if !shared.config.logged_in && persona == Persona::Codex {
                return Err(persona::auth_required());
            }
            shared.created += 1;
            let id = format!("fake-{}-{}", shared.config.segment, shared.created);
            Ok(open(shared, &id, params, false, after))
        }
        "session/resume" | "session/load" => {
            if method == "session/resume" {
                ResumeSessionRequest::parse_message(method, params)?;
            } else {
                LoadSessionRequest::parse_message(method, params)?;
            }
            if shared.config.fault("resume_error") {
                return Err(Error::internal_error().data("fault: resume_error"));
            }
            let id = params["sessionId"].as_str().unwrap_or_default().to_owned();
            Ok(open(shared, &id, params, true, after))
        }
        "session/close" => {
            let req = CloseSessionRequest::parse_message(method, params)?;
            // A prompt still in flight is abandoned with the session (K1).
            shared.sessions.remove(&req.session_id.to_string());
            Ok(json!({}))
        }
        "session/set_config_option" => {
            let req = SetSessionConfigOptionRequest::parse_message(method, params)?;
            if shared.config.fault("set_config_error") {
                return Err(Error::internal_error().data("fault: set_config_error"));
            }
            let session = live(shared, &req.session_id.to_string())?;
            let (id, value) = (&params["configId"], &params["value"]);
            let option = session.config.iter_mut().find(|o| o["id"] == *id);
            let valid = |o: &&mut Value| {
                o["options"]
                    .as_array()
                    .is_some_and(|all| all.iter().any(|v| v["value"] == *value))
            };
            let option = option
                .filter(valid)
                .ok_or_else(|| Error::invalid_params().data("unknown option or value"))?;
            option["currentValue"] = value.clone();
            Ok(json!({"configOptions": session.config}))
        }
        _ => Err(Error::method_not_found()),
    }
}

/// Registers a new, resumed or loaded session; the slash commands follow (s01).
fn open(
    shared: &mut Shared,
    id: &str,
    params: &Value,
    resumed: bool,
    after: &mut Vec<(&'static str, Value)>,
) -> Value {
    let persona = shared.config.persona;
    let config = persona::config_options(persona);
    // Codex's resume result carries no session id (s09).
    let shown = (!resumed || persona == Persona::Claude).then_some(id);
    let result = persona::session_result(persona, shown, &config);
    let session = Session {
        cwd: PathBuf::from(params["cwd"].as_str().unwrap_or_default()),
        resumed,
        config,
        inflight: None,
        crashed: false,
        steer: Steer::Persona,
    };
    shared.sessions.insert(id.to_owned(), session);
    after.push((
        "session/update",
        json!({"sessionId": id, "update": persona::commands(persona)}),
    ));
    result
}

/// A session that answers requests: not closed, and not crashed (K6).
pub fn live<'a>(shared: &'a mut Shared, id: &str) -> Result<&'a mut Session, Error> {
    match shared.sessions.get_mut(id) {
        Some(session) if !session.crashed => Ok(session),
        _ => Err(persona::session_not_found()),
    }
}

fn prompt(
    state: &State,
    params: &Value,
    responder: Responder<Value>,
    cx: &ConnectionTo<Client>,
) -> Result<(), Error> {
    let req = match PromptRequest::parse_message("session/prompt", params) {
        Ok(req) => req,
        Err(e) => return responder.respond_with_error(e),
    };
    let id = req.session_id.to_string();
    let mut shared = lock(state);
    let (persona, logged_in) = (shared.config.persona, shared.config.logged_in);
    shared.prompts += 1;
    let number = shared.prompts;
    let session = match live(&mut shared, &id) {
        Ok(session) => session,
        Err(e) => return responder.respond_with_error(e),
    };
    if session.inflight.is_some() {
        eprintln!(
            "cupper-fake-agent: a second prompt arrived on session {id} while one is in flight (K1)"
        );
        std::process::exit(EXIT_SECOND_PROMPT);
    }
    if !logged_in {
        // Claude when logged out: the session exists, the prompt fails (s10).
        return responder.respond_with_error(persona::auth_required());
    }
    let (cancel, cancelled) = watch::channel(false);
    session.inflight = Some((number, cancel));
    let script = Script::new(
        state.clone(),
        cx.clone(),
        persona,
        &id,
        session.cwd.clone(),
        cancelled,
    );
    drop(shared);
    let text = text_of(&req.prompt);
    cx.spawn(script.prompt(text, number, responder))
}

/// `_session/steering` (s06; the outcome per persona, K10, K11), unless a
/// `steer_*` command chose the answer.
fn steering(
    state: &State,
    params: &Value,
    responder: Responder<Value>,
    cx: &ConnectionTo<Client>,
) -> Result<(), Error> {
    let prompt: Vec<ContentBlock> = match serde_json::from_value(params["prompt"].clone()) {
        Ok(prompt) => prompt,
        Err(e) => return responder.respond_with_error(Error::invalid_params().data(e.to_string())),
    };
    let id = params["sessionId"].as_str().unwrap_or_default();
    let mut shared = lock(state);
    let persona = shared.config.persona;
    let session = match live(&mut shared, id) {
        Ok(session) => session,
        Err(e) => return responder.respond_with_error(e),
    };
    let prompt_required = params["_meta"]["steering"]["idleBehavior"] == "promptRequired";
    let outcome = match session.steer {
        // Never answered: the host gives up after 5 s and queues the message.
        Steer::Hang => return Ok(()),
        Steer::Inject => "injected",
        Steer::NewTurn => "startedNewTurn",
        Steer::Persona if session.inflight.is_some() => "injected",
        // Codex ignores `idleBehavior` (K11).
        Steer::Persona if persona == Persona::Claude && prompt_required => "promptRequired",
        Steer::Persona => "startedNewTurn",
    };
    let (_, never) = watch::channel(false);
    let script = Script::new(
        state.clone(),
        cx.clone(),
        persona,
        id,
        session.cwd.clone(),
        never,
    );
    drop(shared);
    let text = text_of(&prompt);
    match outcome {
        "promptRequired" => {
            responder.respond(json!({"outcome": outcome, "reason": "noRunningTurn"}))
        }
        "injected" => {
            responder.respond(json!({"outcome": outcome}))?;
            script.say(&format!("steered:{text}"))
        }
        _ => {
            responder.respond(json!({"outcome": outcome}))?;
            // The steer text runs as a turn that no prompt belongs to (K9).
            cx.spawn(script.later(0, script::parse_or_reply(&text)))
        }
    }
}

fn notification(state: &State, n: &UntypedMessage) {
    if n.method() != "session/cancel" {
        return;
    }
    let id = n.params()["sessionId"].as_str().unwrap_or_default();
    if let Some((_, cancel)) = lock(state)
        .sessions
        .get(id)
        .and_then(|s| s.inflight.as_ref())
    {
        cancel.send_replace(true);
    }
}

fn text_of(prompt: &[ContentBlock]) -> String {
    prompt
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect()
}
