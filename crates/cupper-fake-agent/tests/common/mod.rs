//! A minimal ACP client that drives the fake agent over its stdio pipes:
//! newline-delimited JSON-RPC, written and read as plain JSON so the tests see
//! exactly what crossed the pipe.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(10);

pub struct Fake {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    /// The agent's stdout has closed: `poll` returns at once.
    closed: bool,
    next_id: u64,
    /// Every line the agent wrote, verbatim.
    pub raw: Vec<String>,
    /// The same lines, parsed.
    pub seen: Vec<Value>,
    /// Results for the agent's requests, in order; a request without one
    /// stays unanswered.
    pub answers: VecDeque<Value>,
    /// The session's working directory, a fresh one per agent.
    pub cwd: PathBuf,
    /// The session opened last.
    pub session: String,
}

impl Fake {
    pub fn spawn(args: &[&str]) -> Self {
        Self::spawn_with(args, &[])
    }

    pub fn spawn_with(args: &[&str], env: &[(&str, &str)]) -> Self {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let n = COUNT.fetch_add(1, Ordering::Relaxed);
        let cwd = std::env::temp_dir().join(format!("cupper-fake-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&cwd).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_cupper-fake-agent"))
            .args(args)
            .envs(env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            stdout
                .lines()
                .map_while(Result::ok)
                .try_for_each(|l| tx.send(l))
        });
        let stdin = child.stdin.take().unwrap();
        let (raw, seen, answers) = (Vec::new(), Vec::new(), VecDeque::new());
        let (cwd, session) = (cwd.canonicalize().unwrap(), String::new());
        Fake {
            child,
            stdin,
            lines,
            closed: false,
            next_id: 0,
            raw,
            seen,
            answers,
            cwd,
            session,
        }
    }

    /// Spawns the agent, initializes it and opens a session.
    pub fn start(args: &[&str]) -> Self {
        let mut fake = Self::spawn(args);
        fake.initialize();
        fake.open(Value::Null);
        fake
    }

    pub fn initialize(&mut self) -> Value {
        let client = json!({"name": "cupper", "version": "0.0.0"});
        let params = json!({"protocolVersion": 1, "clientCapabilities": {}, "clientInfo": client});
        self.call("initialize", params)
    }

    /// `session/new` with `_meta` unless it is null.
    pub fn open(&mut self, meta: Value) -> Value {
        let mut params = json!({"cwd": self.cwd, "mcpServers": []});
        if !meta.is_null() {
            params["_meta"] = meta;
        }
        let response = self.call("session/new", params);
        self.session = response["result"]["sessionId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        response
    }

    pub fn resume(&mut self) -> Value {
        let params = json!({"sessionId": self.session, "cwd": self.cwd, "mcpServers": []});
        self.call("session/resume", params)
    }

    pub fn set_config(&mut self, id: &str, value: &str) -> Value {
        let params = json!({"sessionId": self.session, "configId": id, "value": value});
        self.call("session/set_config_option", params)
    }

    /// Sends a request; returns its id.
    pub fn send(&mut self, method: &str, params: Value) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.write(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        id
    }

    pub fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        self.response(id)
    }

    /// The response to request `id`, the whole message.
    pub fn response(&mut self, id: u64) -> Value {
        let what = format!("response {id}");
        self.wait_for(&what, |m| m["id"] == id && m.get("method").is_none())
    }

    pub fn prompt(&mut self, text: &str) -> Value {
        let id = self.start_prompt(text);
        self.response(id)
    }

    pub fn start_prompt(&mut self, text: &str) -> u64 {
        let prompt = json!([{"type": "text", "text": text}]);
        self.send(
            "session/prompt",
            json!({"sessionId": self.session, "prompt": prompt}),
        )
    }

    /// `_session/steering` with `_meta` unless it is null.
    pub fn steer(&mut self, text: &str, meta: Value) -> u64 {
        let prompt = json!([{"type": "text", "text": text}]);
        let mut params = json!({"sessionId": self.session, "prompt": prompt});
        if !meta.is_null() {
            params["_meta"] = meta;
        }
        self.send("_session/steering", params)
    }

    pub fn cancel(&mut self) {
        let params = json!({"sessionId": self.session});
        self.write(json!({"jsonrpc": "2.0", "method": "session/cancel", "params": params}));
    }

    pub fn write(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// The next message, if one arrives in time. The agent's requests are
    /// answered from `answers`.
    pub fn poll(&mut self, timeout: Duration) -> Option<Value> {
        let line = match self.lines.recv_timeout(timeout) {
            Ok(line) => line,
            Err(RecvTimeoutError::Disconnected) => {
                self.closed = true;
                return None;
            }
            Err(RecvTimeoutError::Timeout) => return None,
        };
        let message: Value = serde_json::from_str(&line).unwrap();
        if message.get("method").is_some()
            && message.get("id").is_some()
            && let Some(result) = self.answers.pop_front()
        {
            self.write(json!({"jsonrpc": "2.0", "id": message["id"], "result": result}));
        }
        self.raw.push(line);
        self.seen.push(message.clone());
        Some(message)
    }

    /// Waits for a new message that matches.
    pub fn wait_for(&mut self, what: &str, matches: impl Fn(&Value) -> bool) -> Value {
        loop {
            match self.poll(WAIT) {
                Some(message) if matches(&message) => return message,
                Some(_) => {}
                None => panic!("no {what}; seen: {:#?}", self.seen),
            }
        }
    }

    /// True if no matching message arrives within `ms`. The agent must stay
    /// up: once its stdout has closed nothing can arrive, and one that exited
    /// would pass for quiet (T189).
    pub fn quiet(&mut self, ms: u64, matches: impl Fn(&Value) -> bool) -> bool {
        while let Some(message) = self.poll(Duration::from_millis(ms)) {
            if matches(&message) {
                return false;
            }
        }
        assert!(
            !self.closed,
            "the agent's stdout closed: it exited, it is not quiet"
        );
        true
    }

    /// Index in `seen` of the first message that matches.
    pub fn index(&self, matches: impl Fn(&Value) -> bool) -> usize {
        let found = self.seen.iter().position(matches);
        found.unwrap_or_else(|| panic!("not seen; seen: {:#?}", self.seen))
    }

    /// The first message seen that matches.
    pub fn find(&self, matches: impl Fn(&Value) -> bool) -> &Value {
        &self.seen[self.index(matches)]
    }

    /// The session updates of one kind seen so far.
    pub fn updates(&self, kind: &str) -> Vec<Value> {
        let updates = self.seen.iter().map(|m| m["params"]["update"].clone());
        updates.filter(|u| u["sessionUpdate"] == kind).collect()
    }

    /// The text of the message chunks seen so far.
    pub fn texts(&self) -> Vec<String> {
        let chunks = self.updates("agent_message_chunk");
        chunks
            .iter()
            .map(|u| u["content"]["text"].as_str().unwrap().to_owned())
            .collect()
    }

    /// Waits for the agent to exit, 10 s at most; its exit code and stderr.
    pub fn exit(&mut self) -> (Option<i32>, String) {
        // The agent closes its stdout on its way out, and from then on `poll`
        // returns at once: a channel nobody sends on paces the tries, or the
        // 200 of them would pass in a few ms, before a loaded machine had the
        // agent exit (T189).
        let (_never, pace) = channel::<()>();
        for _ in 0..200 {
            if let Some(status) = self.child.try_wait().unwrap() {
                let mut stderr = String::new();
                self.child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut stderr)
                    .unwrap();
                return (status.code(), stderr);
            }
            if self.poll(Duration::from_millis(50)).is_none() && self.closed {
                let _ = pace.recv_timeout(Duration::from_millis(50));
            }
        }
        panic!("the agent did not exit");
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.cwd);
    }
}

/// The values of `key` in a list of objects.
pub fn ids(list: &Value, key: &str) -> Vec<String> {
    let list = list.as_array().into_iter().flatten();
    list.filter_map(|o| o[key].as_str().map(str::to_owned))
        .collect()
}

/// A selected permission option, as the client answers it.
pub fn selected(option: &str) -> Value {
    json!({"outcome": {"outcome": "selected", "optionId": option}})
}

pub fn update(m: &Value) -> &Value {
    &m["params"]["update"]
}

/// Matches a message chunk with this text.
pub fn says(text: &str) -> impl Fn(&Value) -> bool + '_ {
    move |m| update(m)["content"]["text"] == text
}

/// Matches a session update of this kind.
pub fn is(kind: &str) -> impl Fn(&Value) -> bool + '_ {
    move |m| update(m)["sessionUpdate"] == kind
}

/// Matches a request from the agent.
pub fn request(method: &str) -> impl Fn(&Value) -> bool + '_ {
    move |m| m["method"] == method && m.get("id").is_some()
}

/// Codex's run state in a message, or null.
pub fn status(m: &Value) -> &Value {
    &update(m)["_meta"]["codex"]["threadStatus"]
}

pub fn stop(response: &Value) -> &Value {
    &response["result"]["stopReason"]
}
