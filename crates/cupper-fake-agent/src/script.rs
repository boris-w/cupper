//! The prompt text is the script (design 5.9.2, part 2): commands separated by
//! `;`. A prompt with an unknown command is not a script and is answered with
//! `fake reply to: <text>`. Interactions and tool shapes follow the persona.
//!
//! | command | effect |
//! |---|---|
//! | `say <text>` / `think <text>` | one message / thought chunk |
//! | `flood <n> <len>` / `flood_mixed <n> <len>` | `n` chunks of `len` chars at full speed (mixed: every other one a thought) |
//! | `big <bytes>` | one message chunk of `bytes` chars |
//! | `tool <id> <kind> <title>` / `long_tool <id> <len>` / `tool_done <id> <bytes>` | start a tool call / with a `len`-char title / complete it with output |
//! | `permission <tool_id> [execute\|edit]` | ask for permission, then `say permission:<option>` |
//! | `question <prompt>` | a one-field form, then `say answer:<value>` |
//! | `sleep <ms>` / `wait_cancel` | wait / until `session/cancel` |
//! | `whoami` / `probe` | say the session, inherited fds, env names |
//! | `spawn_child` / `exit <code>` | start a grandchild in our process group / exit now |

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use agent_client_protocol::{Client, ConnectionTo, Error, Responder, UntypedMessage};
use futures::future::BoxFuture;
use serde_json::{Value, json};
use tokio::sync::{oneshot, watch};

use crate::agent::{State, lock};
use crate::config::Persona::{self, Claude, Codex};
use crate::persona;

const COMMANDS: &[&str] = &[
    "say",
    "think",
    "flood",
    "flood_mixed",
    "big",
    "tool",
    "long_tool",
    "tool_done",
    "permission",
    "question",
    "sleep",
    "wait_cancel",
    "whoami",
    "probe",
    "spawn_child",
    "exit",
];

pub type Command = (String, String);

/// Splits a prompt into commands, or `None` when it is not a script. `;` ends
/// a command.
pub fn parse(text: &str) -> Option<Vec<Command>> {
    let mut commands = Vec::new();
    let mut rest = text;
    loop {
        rest = rest.trim_start_matches(|c: char| c == ';' || c.is_whitespace());
        if rest.is_empty() {
            return Some(commands);
        }
        let (name, after) = rest.split_at(
            rest.find(|c: char| c == ';' || c.is_whitespace())
                .unwrap_or(rest.len()),
        );
        if !COMMANDS.contains(&name) {
            return None;
        }
        let after = after.trim_start_matches([' ', '\t']);
        let len = after.find(';').unwrap_or(after.len());
        commands.push((name.to_owned(), after[..len].trim().to_owned()));
        rest = &after[len..];
    }
}

/// The commands of `text`, or a reply when it is not a script.
pub fn parse_or_reply(text: &str) -> Vec<Command> {
    parse(text).unwrap_or_else(|| vec![("say".to_owned(), format!("fake reply to: {text}"))])
}

/// How a script ends.
enum End {
    Stop(&'static str),
    Fail(Error),
}

enum Flow {
    Next,
    End(End),
}

pub struct Script {
    state: State,
    cx: ConnectionTo<Client>,
    persona: Persona,
    session: String,
    cwd: PathBuf,
    cancel: watch::Receiver<bool>,
}

impl Script {
    pub fn new(
        state: State,
        cx: ConnectionTo<Client>,
        persona: Persona,
        session: &str,
        cwd: PathBuf,
        cancel: watch::Receiver<bool>,
    ) -> Self {
        let session = session.to_owned();
        Script {
            state,
            cx,
            persona,
            session,
            cwd,
            cancel,
        }
    }

    /// Plays a prompt and answers it. Each persona pushes its slash commands
    /// at the start of a turn; Codex also brackets it with its run state.
    pub async fn prompt(
        mut self,
        text: String,
        number: u64,
        responder: Responder<Value>,
    ) -> Result<(), Error> {
        self.update(persona::commands(self.persona))?;
        if self.persona == Codex {
            self.update(persona::thread_status("active", &[]))?;
        }
        let result = match self.run(parse_or_reply(&text)).await {
            End::Fail(e) => Err(e),
            End::Stop(reason) => {
                if self.persona == Codex {
                    self.update(persona::thread_status("idle", &[]))?;
                }
                Ok(json!({"stopReason": reason}))
            }
        };
        // Out of flight before the answer: a prompt sent right after it is not a second one.
        if let Some(session) = lock(&self.state).sessions.get_mut(&self.session)
            && session.inflight.as_ref().is_some_and(|(n, _)| *n == number)
        {
            session.inflight = None;
        }
        responder.respond_with_result(result)
    }

    /// Runs `commands` after `ms`, outside any prompt (K9).
    pub fn later(
        mut self,
        ms: u64,
        commands: Vec<Command>,
    ) -> BoxFuture<'static, Result<(), Error>> {
        Box::pin(async move {
            real_sleep(ms).await;
            self.run(commands).await;
            Ok(())
        })
    }

    async fn run(&mut self, commands: Vec<Command>) -> End {
        for (name, args) in commands {
            if *self.cancel.borrow() {
                return End::Stop("cancelled");
            }
            match self.exec(&name, &args).await {
                Ok(Flow::Next) => {}
                Ok(Flow::End(end)) => return end,
                Err(e) => return End::Fail(e),
            }
        }
        End::Stop(if *self.cancel.borrow() {
            "cancelled"
        } else {
            "end_turn"
        })
    }

    async fn exec(&mut self, name: &str, rest: &str) -> Result<Flow, Error> {
        let args: Vec<&str> = rest.split_whitespace().collect();
        let arg = |i: usize| args.get(i).copied().unwrap_or("");
        let num = |i: usize| arg(i).parse::<u64>().unwrap_or(0);
        match name {
            "say" => self.say(rest)?,
            "think" => self.update(chunk("agent_thought_chunk", rest))?,
            "flood" | "flood_mixed" => {
                for i in 0..num(0) {
                    let thought = name == "flood_mixed" && i % 2 == 1;
                    let kind = if thought {
                        "agent_thought_chunk"
                    } else {
                        "agent_message_chunk"
                    };
                    self.update(chunk(kind, &filler(num(1), i)))?;
                }
            }
            "big" => self.say(&filler(num(0), 0))?,
            "tool" => {
                let title = args.get(2..).unwrap_or_default().join(" ");
                let kind = if ["read", "edit", "execute", "search"].contains(&arg(1)) {
                    arg(1)
                } else {
                    "other"
                };
                self.update(json!({"sessionUpdate": "tool_call", "toolCallId": arg(0), "title": title, "kind": kind, "status": "pending"}))?;
            }
            "long_tool" => {
                let title = filler(num(1), 23);
                self.update(json!({"sessionUpdate": "tool_call", "toolCallId": arg(0), "title": title, "kind": "other", "status": "pending"}))?;
            }
            "tool_done" => {
                let output = json!([{"type": "content", "content": {"type": "text", "text": filler(num(1), 0)}}]);
                self.update(json!({"sessionUpdate": "tool_call_update", "toolCallId": arg(0), "status": "completed", "content": output}))?;
            }
            "permission" => {
                let kind = if arg(1).is_empty() { "execute" } else { arg(1) };
                return self.permission(arg(0), kind).await;
            }
            "question" => {
                let schema = json!({"type": "object", "properties": {"answer": {"type": "string", "title": "Answer"}}, "required": ["answer"]});
                let params = json!({"mode": "form", "sessionId": self.session, "message": rest, "requestedSchema": schema});
                let Some(answer) = self.ask("elicitation/create", params).await? else {
                    return Ok(cancelled());
                };
                let value = match answer["action"].as_str() {
                    Some("accept") => answer["content"]["answer"].as_str().unwrap_or("empty"),
                    other => other.unwrap_or("invalid"),
                };
                self.say(&format!("answer:{value}"))?;
            }
            "sleep" => {
                if self.wait(num(0)).await {
                    return Ok(cancelled());
                }
            }
            "wait_cancel" => {
                self.cancelled().await;
                return Ok(cancelled());
            }
            "whoami" => {
                let (resumed, segment) = {
                    let shared = lock(&self.state);
                    (
                        shared
                            .sessions
                            .get(&self.session)
                            .is_some_and(|s| s.resumed),
                        shared.config.segment,
                    )
                };
                self.say(&format!(
                    "session={} resumed={resumed} segment={segment}",
                    self.session
                ))?;
            }
            "probe" => {
                let fds = INHERITED_FDS.get().map_or("?", String::as_str);
                self.say(&format!("fds={fds} env={}", env_names()))?;
            }
            "spawn_child" => {
                // A CLI child that outlives its adapter; the host must reap it by process group.
                std::process::Command::new("/bin/sleep")
                    .arg("3600")
                    .spawn()
                    .map_err(|e| Error::internal_error().data(e.to_string()))?;
            }
            "exit" => std::process::exit(i32::try_from(num(0)).unwrap_or(1)),
            other => return Err(Error::internal_error().data(format!("no command `{other}`"))),
        }
        Ok(Flow::Next)
    }

    async fn permission(&mut self, tool: &str, kind: &str) -> Result<Flow, Error> {
        let p = self.persona;
        if p == Codex {
            self.update(persona::thread_status("active", &["waitingOnApproval"]))?;
        }
        let params = persona::permission(p, &self.session, tool, kind, &self.cwd);
        let Some(answer) = self.ask("session/request_permission", params).await? else {
            return Ok(cancelled());
        };
        if p == Codex {
            self.update(persona::thread_status("active", &[]))?;
        }
        let picked = answer["outcome"]["optionId"]
            .as_str()
            .unwrap_or("cancelled");
        self.say(&format!("permission:{picked}"))?;
        if persona::rejects(p, picked) {
            self.update(json!({"sessionUpdate": "tool_call_update", "toolCallId": tool, "status": "failed"}))?;
            // Codex's only rejection ends the turn (#556, K2).
            if p == Codex {
                return Ok(cancelled());
            }
        }
        Ok(Flow::Next)
    }

    /// Sends a request to the client and waits for the answer. When the turn
    /// is cancelled first, Claude withdraws the request with `$/cancel_request`
    /// (K5) and Codex leaves it pending (K4); `None` then.
    async fn ask(&mut self, method: &str, params: Value) -> Result<Option<Value>, Error> {
        let sent = self.cx.send_request(UntypedMessage::new(method, params)?);
        let withdraw = sent.cancellation_handle();
        let (tx, rx) = oneshot::channel();
        sent.on_receiving_result(async move |result| {
            let _ = tx.send(result);
            Ok(())
        })?;
        tokio::select! {
            answer = rx => answer.map_err(|_| Error::internal_error().data("no answer"))?.map(Some),
            () = self.cancelled() => {
                if self.persona == Claude {
                    withdraw.cancel()?;
                }
                Ok(None)
            }
        }
    }

    /// Returns true if the turn was cancelled while waiting.
    async fn wait(&mut self, ms: u64) -> bool {
        tokio::select! {
            () = real_sleep(ms) => false,
            () = self.cancelled() => true,
        }
    }

    /// Resolves once the turn is cancelled; never outside a prompt.
    async fn cancelled(&mut self) {
        if self.cancel.wait_for(|c| *c).await.is_err() {
            std::future::pending::<()>().await;
        }
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), Error> {
        self.cx
            .send_notification(UntypedMessage::new(method, params)?)
    }

    fn update(&self, update: Value) -> Result<(), Error> {
        self.notify(
            "session/update",
            json!({"sessionId": self.session, "update": update}),
        )
    }

    pub fn say(&self, text: &str) -> Result<(), Error> {
        self.update(chunk("agent_message_chunk", text))
    }
}

fn cancelled() -> Flow {
    Flow::End(End::Stop("cancelled"))
}

fn chunk(kind: &str, text: &str) -> Value {
    json!({"sessionUpdate": kind, "content": {"type": "text", "text": text}})
}

/// Deterministic text so tests can check content: `len` chars of one letter.
fn filler(len: u64, i: u64) -> String {
    let letter = char::from(b'a' + u8::try_from(i % 26).unwrap_or(0));
    std::iter::repeat_n(letter, usize::try_from(len).unwrap_or(0)).collect()
}

fn env_names() -> String {
    let mut names: Vec<String> = std::env::vars_os()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    names.sort();
    names.join(",")
}

/// File descriptors open when the process started, i.e. the ones it inherited.
static INHERITED_FDS: OnceLock<String> = OnceLock::new();

pub fn record_inherited_fds() {
    INHERITED_FDS.get_or_init(|| {
        // Opening /dev/null takes the lowest free fd; reading /dev/fd reuses it.
        let probe = std::fs::File::open("/dev/null")
            .map(|f| std::os::fd::AsRawFd::as_raw_fd(&f))
            .unwrap_or(-1);
        let mut fds: Vec<i32> = std::fs::read_dir("/dev/fd")
            .map(|d| {
                d.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        fds.retain(|fd| *fd != probe);
        fds.sort_unstable();
        fds.iter().map(i32::to_string).collect::<Vec<_>>().join(",")
    });
}

/// Real time: the fake agent is a separate process that plays a real adapter,
/// so it does not run on the host's `Clock`.
#[expect(
    clippy::disallowed_methods,
    reason = "the fake agent is an external process playing a real adapter in real time"
)]
async fn real_sleep(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}
