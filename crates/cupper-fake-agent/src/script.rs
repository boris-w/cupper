//! The prompt text is the script (design 5.9.2, part 2): commands separated by
//! `;`. A prompt with an unknown command is not a script and is answered with
//! `fake reply to: <text>`.
//!
//! | command | effect |
//! |---|---|
//! | `say <text>` | one message chunk |

use std::time::Duration;

use agent_client_protocol::{Client, ConnectionTo, Error, Responder, UntypedMessage};
use futures::future::BoxFuture;
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::agent::{State, lock};
use crate::config::Persona::{self, Codex};
use crate::persona;

const COMMANDS: &[&str] = &["say"];

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

pub struct Script {
    state: State,
    cx: ConnectionTo<Client>,
    persona: Persona,
    session: String,
    cancel: watch::Receiver<bool>,
}

impl Script {
    pub fn new(
        state: State,
        cx: ConnectionTo<Client>,
        persona: Persona,
        session: &str,
        cancel: watch::Receiver<bool>,
    ) -> Self {
        let session = session.to_owned();
        Script {
            state,
            cx,
            persona,
            session,
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
            if let Err(e) = self.exec(&name, &args).await {
                return End::Fail(e);
            }
        }
        End::Stop(if *self.cancel.borrow() {
            "cancelled"
        } else {
            "end_turn"
        })
    }

    async fn exec(&mut self, name: &str, rest: &str) -> Result<(), Error> {
        match name {
            "say" => self.say(rest),
            other => Err(Error::internal_error().data(format!("no command `{other}`"))),
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

fn chunk(kind: &str, text: &str) -> Value {
    json!({"sessionUpdate": kind, "content": {"type": "text", "text": text}})
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
