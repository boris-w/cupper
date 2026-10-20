//! The prompt text is the script (design 5.9.2, part 2): commands separated by
//! `;`. A prompt with an unknown command is not a script and is answered with
//! `fake reply to: <text>`. Interactions and tool shapes follow the persona.
//!
//! | command | effect |
//! |---|---|
//! | `say <text>` / `think <text>` | one message / thought chunk |
//! | `flood <n> <len>` / `flood_mixed <n> <len>` | `n` chunks of `len` chars at full speed (mixed: every other one a thought) |
//! | `flood_rate <per_s> <s> <len>` | chunks of `len` chars at a fixed rate for `s` seconds |
//! | `big <bytes>` | one message chunk of `bytes` chars |
//! | `tool <id> <kind> <title>` / `long_tool <id> <len>` / `tool_done <id> <bytes>` | start a tool call / with a `len`-char title / complete it with output |
//! | `terminal <id> <exit> <lines>` | a command with `_meta.terminal_output` and `terminal_exit` |
//! | `diff <id> <path>` | an edit tool call with a diff |
//! | `write_file <path> <content>` | write a file inside the session's cwd (`\n` is a newline) |
//! | `permission <tool_id> [execute\|edit]` | ask for permission, then `say permission:<option>` |
//! | `question <prompt>` | a one-field form, then `say answer:<value>` |
//! | `form <q>=<a>\|<b> ...` | a multi-question form, then `say form:<content>` |
//! | `plan_update <markdown>` / `plan_confirm` | a plan / its confirmation, then `say plan:<option>` |
//! | `steer_ok` / `steer_new_turn` / `steer_hang` | how later `_session/steering` requests are answered |
//! | `sleep <ms>` / `wait_cancel` / `hang` | wait / until `session/cancel` / forever, ignoring cancel |
//! | `late_tool <id> <ms>` | start a command; once cancelled, its output arrives `ms` later |
//! | `autonomous <ms>` | end the prompt; the rest of the script runs `ms` later, outside any prompt |
//! | `crash_cli` / `output_then_crash` | the CLI dies (`-32603`, then "Session not found" until resume) / after some output |
//! | `restartable` | the app-server dies (`1001`, `restartable: true`) |
//! | `upstream_retry <n>` / `thread_status <type> [flag...]` | Codex's retry notices / run state |
//! | `auth_none` | `_auth/status_update {kind: none}`, then `-32000` |
//! | `whoami` / `echo_env` / `echo_meta` / `probe` | say the session, cwd, clientInfo, env names, `_meta`, inherited fds |
//! | `spawn_child` / `exit <code>` | start a grandchild in our process group / exit now |
//! | `spawn_group_child [program]` | start `program` (`/bin/sleep`) for an hour in a new process group, holding our stdout; say its pid |
//! | `raw <json>` | write the JSON value as one line, verbatim (`${session}` is the session id) |

use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use agent_client_protocol::{Client, ConnectionTo, Error, Responder, UntypedMessage};
use futures::future::BoxFuture;
use serde_json::{Value, json};
use tokio::sync::{oneshot, watch};

use crate::agent::{self, RAW_METHOD, State, Steer, lock};
use crate::config::Persona::{self, Claude, Codex};
use crate::persona;

const COMMANDS: &[&str] = &[
    "say",
    "think",
    "flood",
    "flood_mixed",
    "flood_rate",
    "big",
    "tool",
    "long_tool",
    "tool_done",
    "terminal",
    "diff",
    "write_file",
    "permission",
    "question",
    "form",
    "plan_update",
    "plan_confirm",
    "steer_ok",
    "steer_new_turn",
    "steer_hang",
    "sleep",
    "wait_cancel",
    "hang",
    "late_tool",
    "autonomous",
    "crash_cli",
    "output_then_crash",
    "restartable",
    "upstream_retry",
    "thread_status",
    "auth_none",
    "whoami",
    "echo_env",
    "echo_meta",
    "probe",
    "spawn_child",
    "spawn_group_child",
    "exit",
    "raw",
];

pub type Command = (String, String);

/// Splits a prompt into commands, or `None` when it is not a script. `;` ends
/// a command, except inside the JSON value of `raw`.
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
        let len = if name == "raw" {
            let mut values = serde_json::Deserializer::from_str(after).into_iter::<Value>();
            values.next()?.ok()?;
            let len = values.byte_offset();
            let tail = after[len..].trim_start();
            if !(tail.is_empty() || tail.starts_with(';')) {
                return None;
            }
            len
        } else {
            after.find(';').unwrap_or(after.len())
        };
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
    /// The prompt never returns.
    Hang,
}

enum Flow {
    Next,
    End(End),
    /// End the prompt; run the rest of the script after this many ms.
    Detach(u64),
}

#[derive(Clone)]
pub struct Script {
    state: State,
    cx: ConnectionTo<Client>,
    persona: Persona,
    session: String,
    cwd: PathBuf,
    cancel: watch::Receiver<bool>,
    /// The latest `plan_update`, shown by `plan_confirm`.
    plan: String,
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
            plan: String::new(),
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
            End::Hang => return Ok(()),
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
        let mut commands = commands.into_iter();
        while let Some((name, args)) = commands.next() {
            if *self.cancel.borrow() {
                return End::Stop("cancelled");
            }
            match self.exec(&name, &args).await {
                Ok(Flow::Next) => {}
                Ok(Flow::End(end)) => return end,
                Ok(Flow::Detach(ms)) => {
                    let later = self.clone().later(ms, commands.collect());
                    return match self.cx.spawn(later) {
                        Ok(()) => End::Stop("end_turn"),
                        Err(e) => End::Fail(e),
                    };
                }
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
        let p = self.persona;
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
            "flood_rate" => return self.flood_rate(num(0), num(1), num(2)).await,
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
            "terminal" => {
                let (id, code) = (arg(0), arg(1).parse::<i64>().unwrap_or(0));
                let lines: Vec<String> = (1..=num(2)).map(|i| format!("line {i}\n")).collect();
                self.update(persona::terminal_start(p, id, &self.cwd))?;
                match p {
                    // Claude gives all output at the end, after an "Exit code" line on failure (s03).
                    Claude => {
                        let head = if code == 0 {
                            String::new()
                        } else {
                            format!("Exit code {code}\n")
                        };
                        self.update(persona::terminal_output(
                            id,
                            (head + &lines.concat()).trim_end(),
                        ))?;
                    }
                    Codex => {
                        for line in &lines {
                            self.update(persona::terminal_output(id, line))?;
                        }
                    }
                }
                self.update(persona::terminal_exit(id, code))?;
            }
            "diff" => {
                self.update(persona::diff(p, arg(0), &self.cwd.join(arg(1))))?;
                self.update(json!({"sessionUpdate": "tool_call_update", "toolCallId": arg(0), "status": "completed"}))?;
            }
            "write_file" => self.write_file(rest)?,
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
            "form" => return self.form(&args).await,
            "plan_update" => {
                self.plan = rest.replace("\\n", "\n");
                self.update(persona::plan_update(p, &self.session, &self.plan))?;
            }
            "plan_confirm" => return self.plan_confirm().await,
            "steer_ok" | "steer_new_turn" | "steer_hang" => {
                let steer = match name {
                    "steer_ok" => Steer::Inject,
                    "steer_new_turn" => Steer::NewTurn,
                    _ => Steer::Hang,
                };
                agent::live(&mut lock(&self.state), &self.session)?.steer = steer;
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
            "hang" => return Ok(Flow::End(End::Hang)),
            "late_tool" => {
                let (id, ms, late) = (arg(0).to_owned(), num(1), self.clone());
                self.update(persona::terminal_start(p, &id, &self.cwd))?;
                self.cancelled().await;
                // Codex does not stop the command; its output and end arrive later (s07, K3).
                self.cx.spawn(async move {
                    real_sleep(ms).await;
                    late.update(persona::terminal_output(&id, "late output\n"))?;
                    late.update(persona::terminal_exit(&id, 0))
                })?;
                return Ok(cancelled());
            }
            "autonomous" => return Ok(Flow::Detach(num(0))),
            "crash_cli" | "output_then_crash" => {
                if name == "output_then_crash" {
                    self.say("output before the crash")?;
                }
                agent::live(&mut lock(&self.state), &self.session)?.crashed = true;
                return Ok(Flow::End(End::Fail(persona::cli_crashed())));
            }
            "restartable" => return Ok(Flow::End(End::Fail(persona::app_server_killed()))),
            "upstream_retry" => {
                for attempt in 1..=num(0) {
                    self.update(persona::upstream_retry(attempt))?;
                }
            }
            "thread_status" => self.update(persona::thread_status(
                arg(0),
                args.get(1..).unwrap_or_default(),
            ))?,
            "auth_none" => {
                self.notify("_auth/status_update", persona::auth_status(p, false))?;
                return Ok(Flow::End(End::Fail(persona::auth_required())));
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
            "echo_env" => {
                // Names only: values would leak the environment into test logs.
                let client = lock(&self.state).client_info.to_string();
                let line = format!(
                    "cwd={} clientInfo={client} env={}",
                    self.cwd.display(),
                    env_names()
                );
                self.say(&line)?;
            }
            "echo_meta" => {
                let meta = lock(&self.state)
                    .sessions
                    .get(&self.session)
                    .map(|s| s.meta.to_string())
                    .unwrap_or_default();
                self.say(&format!("meta={meta}"))?;
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
            "spawn_group_child" => {
                // Claude's tool commands run in a session of their own (T003): killing the
                // adapter's group misses them. A new group is the same to the host.
                let program = Some(arg(0))
                    .filter(|p| !p.is_empty())
                    .unwrap_or("/bin/sleep");
                let child = std::process::Command::new(program)
                    .arg("3600")
                    .process_group(0)
                    .spawn()
                    .map_err(|e| Error::internal_error().data(e.to_string()))?;
                self.say(&format!("pid={}", child.id()))?;
            }
            "exit" => std::process::exit(i32::try_from(num(0)).unwrap_or(1)),
            "raw" => self.notify(
                RAW_METHOD,
                json!({"line": rest.replace("${session}", &self.session)}),
            )?,
            other => return Err(Error::internal_error().data(format!("no command `{other}`"))),
        }
        Ok(Flow::Next)
    }

    async fn flood_rate(&mut self, per_second: u64, seconds: u64, len: u64) -> Result<Flow, Error> {
        let period = Duration::from_secs(1) / u32::try_from(per_second.max(1)).unwrap_or(u32::MAX);
        let mut ticks = real_interval(period);
        for i in 0..per_second * seconds {
            tokio::select! {
                _ = ticks.tick() => {}
                () = self.cancelled() => return Ok(cancelled()),
            }
            self.say(&filler(len, i))?;
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

    async fn form(&mut self, specs: &[&str]) -> Result<Flow, Error> {
        let questions: Vec<(&str, Vec<&str>)> = specs
            .iter()
            .map(|spec| {
                let (name, options) = spec.split_once('=').unwrap_or((spec, ""));
                (name, options.split('|').filter(|o| !o.is_empty()).collect())
            })
            .collect();
        let codex = self.persona == Codex;
        if codex {
            self.update(persona::thread_status("active", &["waitingOnUserInput"]))?;
        }
        let params = persona::form(self.persona, &self.session, &questions);
        let Some(answer) = self.ask("elicitation/create", params).await? else {
            return Ok(cancelled());
        };
        if codex {
            self.update(persona::thread_status("active", &[]))?;
        }
        let shown = match answer["action"].as_str() {
            Some("accept") => answer["content"].to_string(),
            other => other.unwrap_or("invalid").to_owned(),
        };
        self.say(&format!("form:{shown}"))?;
        Ok(Flow::Next)
    }

    /// Plan confirmation (s05). Claude's approval switches the mode (ExitPlanMode);
    /// Codex goes idle before asking (K12) and leaves plan mode on approval.
    async fn plan_confirm(&mut self) -> Result<Flow, Error> {
        let p = self.persona;
        let plan = if self.plan.is_empty() {
            "# Plan\n\nA fake plan.".to_owned()
        } else {
            self.plan.clone()
        };
        let tool = "fake-exit-plan";
        let finish = |status: &str| json!({"sessionUpdate": "tool_call_update", "toolCallId": tool, "status": status});
        match p {
            Claude => self.update(json!({"sessionUpdate": "tool_call", "toolCallId": tool, "name": "ExitPlanMode",
                "status": "pending", "title": "Approve Plan", "kind": "switch_mode", "content": []}))?,
            Codex => self.update(persona::thread_status("idle", &[]))?,
        }
        let params = persona::plan_confirm(p, &self.session, &plan);
        let Some(answer) = self.ask("session/request_permission", params).await? else {
            return Ok(cancelled());
        };
        let picked = answer["outcome"]["optionId"]
            .as_str()
            .unwrap_or("cancelled");
        let (option, value) = match picked {
            "exit-plan-default" => ("mode", "default"),
            "exit-plan-auto" | "exit-plan-clear-auto" => ("mode", "auto"),
            "exit-plan-bypass" => ("mode", "bypassPermissions"),
            "implement_plan" => ("collaboration_mode", "default"),
            _ => ("", ""),
        };
        if option.is_empty() {
            if p == Claude {
                self.update(finish("failed"))?;
            }
        } else {
            let config = self.set_config(option, value)?;
            if p == Claude {
                self.update(
                    json!({"sessionUpdate": "current_mode_update", "currentModeId": value}),
                )?;
            }
            self.update(json!({"sessionUpdate": "config_option_update", "configOptions": config}))?;
            match p {
                Claude => {
                    self.update(finish("completed"))?;
                    self.update(persona::plan_update(p, &self.session, &plan))?;
                }
                Codex => self.update(persona::thread_status("active", &[]))?,
            }
        }
        self.say(&format!("plan:{picked}"))?;
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

    /// Writes a file inside the session's cwd: no absolute path, no `..`, and
    /// no symbolic link that leads out of it.
    fn write_file(&self, rest: &str) -> Result<(), Error> {
        let (path, content) = rest.split_once(' ').unwrap_or((rest, ""));
        let refuse =
            |why: &str| Error::invalid_params().data(format!("write_file `{path}`: {why}"));
        let io = |e: std::io::Error| refuse(&e.to_string());
        if path.is_empty()
            || !Path::new(path)
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(refuse("not a relative path inside the workspace"));
        }
        let (root, target) = (self.cwd.canonicalize().map_err(io)?, self.cwd.join(path));
        let existing = target
            .ancestors()
            .find(|a| a.symlink_metadata().is_ok())
            .unwrap_or(&self.cwd);
        if !existing.canonicalize().map_err(io)?.starts_with(&root) {
            return Err(refuse("leads out of the workspace"));
        }
        std::fs::create_dir_all(target.parent().unwrap_or(&self.cwd)).map_err(io)?;
        std::fs::write(&target, content.replace("\\n", "\n")).map_err(io)
    }

    /// Sets a config option of this session; returns all of them.
    fn set_config(&self, option: &str, value: &str) -> Result<Vec<Value>, Error> {
        let mut shared = lock(&self.state);
        let session = agent::live(&mut shared, &self.session)?;
        for o in session.config.iter_mut().filter(|o| o["id"] == option) {
            o["currentValue"] = json!(value);
        }
        Ok(session.config.clone())
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

#[expect(
    clippy::disallowed_methods,
    reason = "the fake agent is an external process playing a real adapter in real time"
)]
fn real_interval(period: Duration) -> tokio::time::Interval {
    tokio::time::interval(period)
}
