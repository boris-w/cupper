//! GOLD-A (design 5.9.3): every Claude recording in `testdata/recordings/`
//! whose `usage` has `gold-a` goes through what the runtime puts in front of
//! the extraction layer, the tap's K19 rewrite and the SDK's types, and the
//! result is compared with its golden: the same path under
//! `testdata/golden/gold-a/`. `UPDATE_GOLDEN=1` writes the goldens instead
//! (`cargo xtask ci` refuses to run with it set).
//!
//! A golden has one JSON value per line, keys sorted:
//! - `{"segment": n}` where adapter instance `n` begins;
//! - `{"event": ...}` per driver event; asks and prompt tags count from 1 per
//!   recording, in the order the agent's requests and the client's prompts
//!   came;
//! - `{"capabilities": ...}` for `initialize`, before any session's options
//!   and commands; `{"session": ...}` with the workspace and the options when
//!   a session opens; `{"auth": ...}` with the kind and plan of a login state
//!   (K16); `{"failed": ...}` for a request other than a prompt that failed.
//!
//! Normalization: consecutive text of one message, or of one run of thought,
//! is folded into a single event, since how the agent cuts it depends on the
//! machine's load. IDs, paths and times stay as recorded: they come from a
//! recording that does not change.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use agent_client_protocol::Error;
use agent_client_protocol::schema::v1::{
    CreateElicitationRequest, InitializeResponse, PromptResponse, RequestPermissionRequest,
    SessionConfigOption, SessionNotification,
};
use cupper_protocol::capabilities::{CliCompat, Versions};
use cupper_protocol::item::ToolKind;
use cupper_protocol::recording::{Body, Recording, Usage};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::capabilities::TIERS;
use super::options::{self, Scene};
use super::{ClaudeSession, agent_error, ask, capabilities, prompt_done};
use crate::driver::acp::AuthStatusUpdate;
use crate::driver::api::{AskId, AskRequest, DriverEvent, PromptTag};
use crate::driver::tap::k19;

const ADAPTER: &str = "claude-agent-acp";

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata")
}

/// The files under `dir`, sorted; none when it does not exist.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for path in fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
    {
        match path.is_dir() {
            true => found.extend(files(&path)),
            false => found.push(path),
        }
    }
    found.sort();
    found
}

/// The Claude recordings for GOLD-A, by path under `testdata/recordings/`.
pub(super) fn recordings(scenario: Option<&str>) -> Vec<(PathBuf, Recording)> {
    let base = testdata().join("recordings");
    let named = |path: &PathBuf| scenario.is_none_or(|s| path.file_stem() == Some(s.as_ref()));
    (files(&base.join(ADAPTER)).into_iter().filter(named))
        .map(|path| {
            let text = fs::read_to_string(&path).expect("a recording");
            let recording = Recording::parse(&text).expect("a recording in format v1");
            let name = path.strip_prefix(&base).expect("under the base").to_owned();
            (name, recording)
        })
        .filter(|(_, recording)| recording.header.usage.contains(&Usage::GoldA))
        .collect()
}

/// What the extraction layer makes of `recording`, normalized.
pub(super) fn extract(recording: &Recording) -> Vec<Value> {
    let header = &recording.header;
    let versions = Versions {
        adapter: header.adapter_version.clone(),
        cli: header.cli_version.clone(),
        cli_compat: CliCompat::Verified,
    };
    let mut run = Run::default();
    for (n, segment) in recording.segments().enumerate() {
        run.out.push(json!({"segment": n}));
        // The client's requests of this instance, by JSON-RPC ID.
        let mut requests: HashMap<String, (String, Value)> = HashMap::new();
        for frame in segment {
            match &frame.body {
                Body::Send(msg) => {
                    if let (Some(method), Some(id)) = (msg["method"].as_str(), msg.get("id")) {
                        requests.insert(id.to_string(), (method.to_owned(), msg["params"].clone()));
                    }
                }
                Body::Recv(msg) => {
                    let mut msg = msg.clone();
                    k19::rewrite(&mut msg);
                    match msg["method"].as_str() {
                        Some(method) => run.agent_message(method, &msg),
                        None => {
                            let id = msg.get("id").map(Value::to_string).unwrap_or_default();
                            if let Some((method, params)) = requests.remove(&id) {
                                run.reply(&method, &params, &msg, &versions);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    fold(run.out)
}

#[derive(Default)]
struct Run {
    out: Vec<Value>,
    sessions: HashMap<String, ClaudeSession>,
    /// By the JSON-RPC ID of the agent's request.
    asks: HashMap<String, AskId>,
    prompts: u64,
}

impl Run {
    fn event(&mut self, event: DriverEvent) {
        self.out.push(json!({"event": event}));
    }

    fn session(&mut self, id: impl ToString) -> &mut ClaudeSession {
        let new = || ClaudeSession::new(PathBuf::new(), Vec::new());
        self.sessions.entry(id.to_string()).or_insert_with(new)
    }

    /// A notification or a request from the agent.
    fn agent_message(&mut self, method: &str, msg: &Value) {
        let params = &msg["params"];
        match method {
            "session/update" => {
                if let Some(n) = parse::<SessionNotification>(params) {
                    let events = self.session(&n.session_id).update(&n.update);
                    events.into_iter().for_each(|event| self.event(event));
                }
            }
            "_auth/status_update" => {
                if let Some(AuthStatusUpdate { auth_status: auth }) = parse(params) {
                    let plan = auth.account.and_then(|account| account.plan);
                    self.out
                        .push(json!({"auth": {"kind": auth.kind, "plan": plan}}));
                }
            }
            "session/request_permission" => {
                if let Some(request) = parse::<RequestPermissionRequest>(params) {
                    let (tool_call_id, ask) =
                        self.session(&request.session_id).permission(&request);
                    self.ask(msg, Some(tool_call_id), ask);
                }
            }
            "elicitation/create" => {
                let request = parse::<CreateElicitationRequest>(params);
                if let Some((tool_call_id, ask)) = request.as_ref().and_then(ask::elicitation) {
                    self.ask(msg, tool_call_id, ask);
                }
            }
            "$/cancel_request" => {
                if let Some(&ask) = self.asks.get(&params["requestId"].to_string()) {
                    self.event(DriverEvent::AskWithdrawn(ask));
                }
            }
            _ => {}
        }
    }

    fn ask(&mut self, msg: &Value, tool_call_id: Option<String>, request: AskRequest) {
        let ask = AskId(self.asks.len() as u64 + 1);
        self.asks.insert(msg["id"].to_string(), ask);
        let event = DriverEvent::Ask {
            ask,
            tool_call_id,
            request,
        };
        self.event(event);
    }

    /// The agent's reply to the client's `method` request.
    fn reply(&mut self, method: &str, params: &Value, msg: &Value, versions: &Versions) {
        let tag = (method == "session/prompt").then(|| {
            self.prompts += 1;
            PromptTag(self.prompts)
        });
        if let Some(error) = msg.get("error").and_then(parse::<Error>) {
            let error = agent_error(&error);
            return match tag {
                Some(tag) => self.event(DriverEvent::PromptFailed { tag, error }),
                None => self
                    .out
                    .push(json!({"failed": {"method": method, "error": error}})),
            };
        }
        let result = &msg["result"];
        match method {
            "initialize" => {
                if let Some(init) = parse::<InitializeResponse>(result) {
                    let caps = capabilities::capabilities(&init, versions.clone(), vec![], vec![]);
                    self.out.push(json!({"capabilities": caps}));
                }
            }
            "session/new" | "session/load" | "session/resume" => {
                let id = result["sessionId"]
                    .as_str()
                    .or(params["sessionId"].as_str());
                let options = parse::<Vec<SessionConfigOption>>(&result["configOptions"]);
                let config = capabilities::config_options(&options.unwrap_or_default());
                let cwd = params["cwd"].as_str().unwrap_or_default();
                let session = json!({"session_id": id, "cwd": cwd, "config": config});
                self.out.push(json!({"session": session}));
                let session = ClaudeSession::new(cwd.into(), config);
                self.sessions
                    .insert(id.unwrap_or_default().to_owned(), session);
            }
            "session/set_config_option" => {
                let options = parse::<Vec<SessionConfigOption>>(&result["configOptions"]);
                let id = params["sessionId"].as_str().unwrap_or_default();
                let event = self.session(id).set_config(&options.unwrap_or_default());
                self.event(event);
            }
            "session/prompt" => {
                if let (Some(tag), Some(response)) = (tag, parse::<PromptResponse>(result)) {
                    let (stop_reason, usage) = prompt_done(&response);
                    let done = DriverEvent::PromptDone {
                        tag,
                        stop_reason,
                        usage,
                    };
                    self.event(done);
                }
            }
            _ => {}
        }
    }
}

fn parse<T: DeserializeOwned>(value: &Value) -> Option<T> {
    serde_json::from_value(value.clone()).ok()
}

/// Folds consecutive text of one message, or of one run of thought, and
/// sorts every object's keys.
fn fold(out: Vec<Value>) -> Vec<Value> {
    let text = |value: &Value| {
        ["agent_text", "thought"].into_iter().find_map(|kind| {
            let update = value.pointer(&format!("/event/update/{kind}"))?;
            Some((
                format!("/event/update/{kind}/text"),
                update["message_id"].clone(),
            ))
        })
    };
    let mut folded: Vec<Value> = Vec::new();
    for mut value in out {
        if let (Some(last), Some(next)) = (folded.last_mut(), text(&value))
            && text(last).as_ref() == Some(&next)
            && let (Some(Value::String(to)), Some(Value::String(more))) =
                (last.pointer_mut(&next.0), value.pointer(&next.0))
        {
            to.push_str(more);
            continue;
        }
        value.sort_all_objects();
        folded.push(value);
    }
    folded
}

/// Compares `scenario`'s extraction with its golden, or writes the golden.
fn check(scenario: &str) {
    let found = recordings(Some(scenario));
    assert!(
        !found.is_empty(),
        "no Claude recording of {scenario} for GOLD-A"
    );
    for (name, recording) in found {
        let path = testdata().join("golden/gold-a").join(&name);
        let text: String = (extract(&recording).iter())
            .map(|value| format!("{value}\n"))
            .collect();
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            fs::create_dir_all(path.parent().expect("a directory")).expect("mkdir");
            fs::write(&path, text).expect("write the golden");
            continue;
        }
        let golden = fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("no golden for {name:?}; write it with UPDATE_GOLDEN=1 and review it")
        });
        let cut = |line: &str| line.chars().take(400).collect::<String>();
        let lines = golden.lines().zip(text.lines()).enumerate();
        if let Some((n, (want, got))) = lines.into_iter().find(|(_, (a, b))| a != b) {
            panic!(
                "{name:?} line {}:\n  golden:    {}\n  extracted: {}",
                n + 1,
                cut(want),
                cut(got)
            );
        }
        let counts = (golden.lines().count(), text.lines().count());
        assert_eq!(
            counts.0, counts.1,
            "{name:?}: golden and extraction lengths"
        );
    }
}

macro_rules! gold_a {
    ($($test:ident => $scenario:literal,)*) => {
        $(
            #[test]
            fn $test() {
                check($scenario);
            }
        )*
        const SCENARIOS: &[&str] = &[$($scenario),*];
    };
}

gold_a! {
    gold_a_claude_s01 => "s01-init-session",
    gold_a_claude_s02 => "s02-normal-turn",
    gold_a_claude_s02b => "s02b-plan",
    gold_a_claude_s02c => "s02c-plan-tasks-enabled",
    gold_a_claude_s02d => "s02d-thinking",
    gold_a_claude_s03 => "s03-exec",
    gold_a_claude_s04 => "s04-approval",
    gold_a_claude_s04b => "s04b-reject-command",
    gold_a_claude_s05 => "s05-questions",
    gold_a_claude_s06 => "s06-queue-steer",
    gold_a_claude_s07 => "s07-cancel",
    gold_a_claude_s07b => "s07b-cli-crash-recovery",
    gold_a_claude_s07x => "s07x-sleep-backgrounded",
    gold_a_claude_s08 => "s08-list",
    gold_a_claude_s09 => "s09-load-resume",
    gold_a_claude_s10 => "s10-unauthenticated",
    gold_a_claude_s11 => "s11-image",
    gold_a_claude_s12 => "s12-resources",
}

/// Every Claude recording for GOLD-A has a test above and a golden, and every
/// golden has its recording (design 5.9.3).
#[test]
fn gold_a_claude_recordings_and_goldens_match_up() {
    let recorded: Vec<PathBuf> = recordings(None).into_iter().map(|(name, _)| name).collect();
    for name in &recorded {
        let scenario = name
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        assert!(SCENARIOS.contains(&scenario), "{name:?} has no GOLD-A test");
        let golden = testdata().join("golden/gold-a").join(name);
        assert!(golden.is_file(), "{name:?} has no golden");
    }
    let goldens = testdata().join("golden/gold-a");
    let paths = files(&goldens.join(ADAPTER));
    for path in paths {
        let name = path.strip_prefix(&goldens).expect("under the goldens");
        assert!(
            recorded.iter().any(|r| r == name),
            "{name:?} has no recording"
        );
    }
}

/// The fields each tool call of `scenario` ends with, from its start and
/// updates, later values winning; in the order the calls started.
fn tool_calls(scenario: &str) -> Vec<Value> {
    let mut calls: Vec<Value> = Vec::new();
    for value in recordings(Some(scenario))
        .iter()
        .flat_map(|(_, r)| extract(r))
    {
        let update = |kind| value.pointer(&format!("/event/update/{kind}"));
        let Some(Value::Object(fields)) = update("tool_start").or(update("tool_update")) else {
            continue;
        };
        let id = &fields["tool_call_id"];
        let call = match calls.iter_mut().find(|call| &call["tool_call_id"] == id) {
            Some(call) => call,
            None => calls.push_mut(json!({})),
        };
        let fields = fields.iter().filter(|(_, v)| !v.is_null());
        call.as_object_mut()
            .expect("an object")
            .extend(fields.map(|(k, v)| (k.clone(), v.clone())));
    }
    calls
}

/// Commands get their text from an update, the workspace, their output
/// without the "Exit code" line, and their exit code; edits get their paths,
/// relative to the workspace, and their diffs (design 5.6.3).
#[test]
fn gold_a_tool_fields_claude_of_commands_and_edits() {
    let commands: Vec<Value> = (tool_calls("s03-exec").iter())
        .map(|c| {
            json!([
                c["command"],
                c["cwd"],
                c["output"],
                c["exit_code"],
                c["status"]
            ])
        })
        .collect();
    let cwd = "<tmp>/repos/claude";
    let sh = "sh -c 'echo out; echo err 1>&2; exit 3'";
    assert_eq!(
        commands,
        [
            json!(["ls", cwd, {"replace": "app.js\nhello.txt\nREADME.md"}, 0, "completed"]),
            json!([sh, cwd, {"replace": "out\nerr"}, 3, "failed"]),
        ]
    );
    let edits: Vec<Value> = (tool_calls("s02-normal-turn").iter())
        .filter(|call| call["tool_kind"] == "edit")
        .map(|call| {
            json!([
                call["paths"],
                call["diffs"][0]["change"],
                call["diffs"][0]["stat"]
            ])
        })
        .collect();
    let stat = |added, removed| json!({"added": added, "removed": removed});
    assert_eq!(
        edits,
        [
            json!([["hello.txt"], "add", stat(1, 0)]),
            json!([["app.js"], "update", stat(1, 1)]),
            json!([["app.js"], "update", stat(1, 1)]),
        ]
    );
}

/// Enum values the SDK does not know (K19) pass the tap's rewrite, and the
/// extraction layer decides on the originals; an update type it does not
/// know yields nothing.
#[test]
fn gold_a_unknown_enums_are_judged_by_their_originals() {
    let text = r#"{"format":"cupper-rec","v":1,"source":"host","adapter":"claude-agent-acp","adapter_version":"0.91.0","cli":"claude","cli_version":"2.1.296","scenario":"unknown-enums","usage":["gold-a"]}
{"seq":1,"t":0,"instance":0,"dir":"note","note":{"event":"instance_started"}}
{"seq":2,"t":0,"instance":0,"dir":"send","msg":{"jsonrpc":"2.0","id":1,"method":"session/prompt","params":{"sessionId":"s","prompt":[]}}}
{"seq":3,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"t","title":"x","status":"blocked"}}}}
{"seq":4,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"weather_report"}}}}
{"seq":5,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","id":7,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"t"},"options":[{"optionId":"o","name":"Always","kind":"allow_forever"}]}}}
{"seq":6,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","id":1,"result":{"stopReason":"paused"}}}"#;
    let out = extract(&Recording::parse(text).expect("a recording"));
    let [_, tool, ask, done] = &out[..] else {
        panic!("{out:?}")
    };
    let tool = &tool["event"]["update"]["tool_update"];
    assert_eq!(
        (&tool["title"], &tool["status"]),
        (&json!("x"), &Value::Null)
    );
    let option = &ask["event"]["ask"]["request"]["classified"]["options"][0];
    assert_eq!(
        (&option["kind"], &option["requires"], &option["effect"]),
        (
            &json!("allow_forever"),
            &json!("privileged"),
            &json!({"persistent": true, "rule": {"prefix_risk": "unknown"}, "scope": "unknown"})
        )
    );
    assert_eq!(done["event"]["prompt_done"]["stop_reason"], "paused");
}

/// The values of `recording` that the tables do not have (design 5.9.5):
/// options by scene, tool kinds and permission modes.
fn not_in_tables(recording: &Recording) -> Vec<String> {
    let mut missing = Vec::new();
    for frame in &recording.frames {
        let Body::Recv(msg) = &frame.body else {
            continue;
        };
        let mut msg = msg.clone();
        k19::rewrite(&mut msg);
        let (params, update) = (&msg["params"], &msg["params"]["update"]);
        if msg["method"] == "session/request_permission"
            && let Some(request) = parse::<RequestPermissionRequest>(params)
        {
            let scene = Scene::of(&request);
            let unlisted = request
                .options
                .iter()
                .filter(|o| !options::listed(scene, o));
            missing.extend(unlisted.map(|o| format!("option {} in {scene:?}", o.option_id)));
        }
        for kind in [&update["kind"], &params["toolCall"]["kind"]].map(Value::as_str) {
            if let Some(ToolKind::Unknown(kind)) = kind.map(ToolKind::from) {
                missing.push(format!("tool kind {kind}"));
            }
        }
        let options = msg["result"]["configOptions"].as_array();
        let modes = (options
            .or(update["configOptions"].as_array())
            .into_iter()
            .flatten())
        .filter(|option| option["category"] == "mode")
        .flat_map(|option| {
            let values = option["options"].as_array().into_iter().flatten();
            let grouped = values
                .clone()
                .flat_map(|v| v["options"].as_array().into_iter().flatten());
            values
                .chain(grouped)
                .map(|v| &v["value"])
                .chain([&option["currentValue"]])
        })
        .chain([&update["currentModeId"]]);
        for mode in modes.filter_map(Value::as_str) {
            if !TIERS.iter().any(|(known, _)| *known == mode) {
                missing.push(format!("permission mode {mode}"));
            }
        }
    }
    missing.sort();
    missing.dedup();
    missing
}

#[test]
fn gold_a_option_map_claude_has_every_recorded_value() {
    for (name, recording) in recordings(None) {
        assert_eq!(not_in_tables(&recording), Vec::<String>::new(), "{name:?}");
    }
}

#[test]
fn gold_a_option_map_claude_flags_values_outside_the_tables() {
    let text = r#"{"format":"cupper-rec","v":1,"source":"host","adapter":"claude-agent-acp","adapter_version":"0.91.0","cli":"claude","cli_version":"2.1.296","scenario":"new-values","usage":["gold-a"]}
{"seq":1,"t":0,"instance":0,"dir":"note","note":{"event":"instance_started"}}
{"seq":2,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","id":1,"result":{"sessionId":"s","configOptions":[{"id":"mode","name":"Mode","category":"mode","type":"select","currentValue":"default","options":[{"value":"default","name":"Default"},{"value":"dontAsk","name":"Don't ask"}]}]}}}
{"seq":3,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"current_mode_update","currentModeId":"yolo"}}}}
{"seq":4,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call","toolCallId":"t","title":"x","kind":"teleport"}}}}
{"seq":5,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","id":7,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"t","name":"Bash"},"options":[{"optionId":"allow-once","name":"Yes","kind":"allow_once"},{"optionId":"allow-everywhere","name":"Always","kind":"allow_always"}]}}}"#;
    let recording = Recording::parse(text).expect("a recording");
    assert_eq!(
        not_in_tables(&recording),
        [
            "option allow-everywhere in Command",
            "permission mode dontAsk",
            "permission mode yolo",
            "tool kind teleport"
        ]
    );
}
