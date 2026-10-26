//! The compat suite's runner (design 5.9.5), which `cargo xtask compat`
//! starts: an ignored test that reads its plan from `CUPPER_COMPAT_PLAN`,
//! plays each scenario on a host of its own and writes what happened, with
//! the session's recording, under the plan's `out`. xtask makes the report
//! from it. The fake agent plays the CLI so far.

mod common;
#[path = "compat/scenarios.rs"]
mod scenarios;

use std::path::{Path, PathBuf};
use std::time::Duration;

use cupper_cli::client::{self, Client};
use cupper_host::clock::{Clock, SystemClock};
use cupper_host::gateway::testkit::TempDir;
use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::methods::admin::DiagRecordingExport;
use cupper_protocol::methods::session::{SessionCreate, SessionInterrupt};
use cupper_protocol::recording::{Recording, Usage};
use serde::Deserialize;
use serde_json::{Value, json};

use common::*;
use scenarios::{Check, SCENARIOS, Scenario, Step};

/// What stands for the project's path in a recording.
const WORK: &str = "/work/project";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    /// The scenarios to play, by ID; every one when empty.
    only: Vec<String>,
    /// How many times a scenario is played while it stays inconclusive.
    attempts: u32,
    out: PathBuf,
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "run by `cargo xtask compat`"]
async fn compat_driver() {
    let plan = std::env::var("CUPPER_COMPAT_PLAN").expect("CUPPER_COMPAT_PLAN: the plan's path");
    let plan: Plan = serde_json::from_str(&std::fs::read_to_string(plan).unwrap()).unwrap();
    for id in &plan.only {
        assert!(SCENARIOS.iter().any(|s| s.id == id), "no scenario {id}");
    }
    std::fs::create_dir_all(plan.out.join("recordings")).unwrap();
    let chosen = |s: &&Scenario| plan.only.is_empty() || plan.only.iter().any(|id| id == s.id);
    let clock = SystemClock::new();
    let (mut results, mut roots) = (Vec::new(), Vec::new());
    for scenario in SCENARIOS.iter().filter(chosen) {
        let started = clock.now_mono();
        let mut attempts = 0;
        let mut played = loop {
            attempts += 1;
            let dir = TempDir::new(&format!("compat-{}", scenario.id));
            let real = std::fs::canonicalize(dir.path()).unwrap();
            roots.extend([dir.path().to_owned(), real]);
            let played = match tokio::spawn(play(scenario, dir, plan.out.clone())).await {
                Ok(played) => played,
                Err(err) => ended("fail", &format!("the runner panicked: {}", panicked(err))),
            };
            if played["outcome"] != "inconclusive" || attempts >= plan.attempts {
                break played;
            }
        };
        let ms = clock
            .now_mono()
            .saturating_duration_since(started)
            .as_millis();
        let more: serde_json::Map<_, _> = typed(json!({"id": scenario.id, "title": scenario.title,
            "gold_b": scenario.gold_b, "attempts": attempts, "duration_ms": ms as u64}));
        played.as_object_mut().unwrap().extend(more);
        results.push(played);
    }
    let run = json!({"adapter": {"name": "cupper-fake-agent", "version": "0.0.0"},
        "cli": {"name": "claude", "version": "fake"}, "roots": roots, "scenarios": results});
    let text = serde_json::to_string_pretty(&run).unwrap();
    std::fs::write(plan.out.join("run.json"), text).unwrap();
}

fn panicked(err: tokio::task::JoinError) -> String {
    let Ok(panic) = err.try_into_panic() else {
        return "cancelled".into();
    };
    let text = panic.downcast_ref::<&str>().map(|s| (*s).to_owned());
    text.or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_default()
}

/// A scenario that did not get as far as its checks.
fn ended(outcome: &str, detail: &str) -> Value {
    json!({"outcome": outcome, "detail": detail, "checks": [], "recording": null})
}

async fn play(scenario: &'static Scenario, dir: TempDir, out: PathBuf) -> Value {
    let (data_dir, host, client) = fake_agent_host(&dir, json!({})).await;
    let mut host = Running(host);
    let session = create(&client, scenario).await;
    let mut user = User {
        data_dir,
        session,
        client,
        asked: 0,
    };
    if let Err(waited) = user.play(scenario.steps).await {
        // The agent did not do what the scenario needs (design 5.9.5).
        return ended("inconclusive", &waited);
    }
    let events = user.events();
    let held = |c| json!({"check": Check::name(c), "passed": holds(c, &events, dir.path())});
    let checks: Vec<_> = scenario.checks.iter().map(held).collect();
    let recording = user.export(scenario, &dir, &out).await;
    terminate(&host.0);
    let code = exit_code(&mut host.0).await;
    let unmet = checks.iter().find(|check| check["passed"] == false);
    let detail = match (code, unmet) {
        (0, None) => None,
        (0, Some(check)) => Some(format!("not met: {}", check["check"].as_str().unwrap())),
        (code, _) => Some(format!("the host exited with {code}")),
    };
    json!({"outcome": if detail.is_some() { "fail" } else { "pass" }, "detail": detail,
        "checks": checks, "recording": recording})
}

/// `session/create` in the scenario's permission mode.
async fn create(client: &Client, scenario: &Scenario) -> SessionId {
    let mut params = json!({"command_id": client::command_id(), "agent": "claude",
        "project_id": ProjectId::from_uuid(1), "workspace": {"kind": "local"},
        "message": [{"type": "text", "text": scenario.script}]});
    if let Some(mode) = scenario.mode {
        params["config"] = json!({"permission_mode": mode});
    }
    let created = client
        .request::<SessionCreate>(typed(params))
        .await
        .unwrap();
    created.session_id
}

struct User {
    data_dir: PathBuf,
    session: SessionId,
    client: Client,
    /// The requests a step has taken.
    asked: usize,
}

impl User {
    /// The steps, then the end of the turn; what it waited for in vain.
    async fn play(&mut self, steps: &[Step]) -> Result<(), String> {
        for step in steps {
            match step {
                Step::Asked => {
                    self.request().await?;
                }
                Step::Choose(kind) => {
                    let asked = self.request().await?;
                    let options = asked["request"]["options"].as_array();
                    let option = options.into_iter().flatten().find(|o| o["kind"] == *kind);
                    let Some(option) = option else {
                        return Err(format!("an option of kind {kind}"));
                    };
                    let chosen = json!({"type": "selected", "option_id": option["option_id"]});
                    respond(&self.client, &self.session, &asked, chosen)
                        .await
                        .unwrap();
                }
                Step::Answer(text) => {
                    let asked = self.request().await?;
                    let fields = asked["request"]["fields"].as_array().into_iter().flatten();
                    let values: serde_json::Map<_, _> = fields.map(|f| answer(f, text)).collect();
                    let answer = json!({"type": "answer", "values": values});
                    respond(&self.client, &self.session, &asked, answer)
                        .await
                        .unwrap();
                }
                Step::Pause(ms) => {
                    let clock = SystemClock::new();
                    clock
                        .timer_at(clock.now_mono() + Duration::from_millis(*ms))
                        .await;
                }
                Step::Interrupt => {
                    let params =
                        json!({"command_id": client::command_id(), "session_id": self.session});
                    self.client
                        .request::<SessionInterrupt>(typed(params))
                        .await
                        .unwrap();
                }
            }
        }
        self.wait("the end of the turn", |events| {
            of(events, "turn.completed").next().map(drop)
        })
        .await
    }

    /// The next request no step has taken yet.
    async fn request(&mut self) -> Result<Value, String> {
        let n = self.asked;
        let asked = self
            .wait("a request", |events| {
                of(events, "interaction.requested").nth(n).cloned()
            })
            .await?;
        self.asked += 1;
        Ok(asked)
    }

    /// Until `find` finds it in the session's events.
    async fn wait<T>(&self, what: &str, find: impl Fn(&[Value]) -> Option<T>) -> Result<T, String> {
        let clock = SystemClock::new();
        let give_up = clock.now_mono() + PATIENCE;
        loop {
            if let Some(found) = find(&self.events()) {
                return Ok(found);
            }
            if clock.now_mono() >= give_up {
                return Err(format!("no {what} within {}s", PATIENCE.as_secs()));
            }
            wait(&clock).await;
        }
    }

    /// The session's events so far, as `{type, data}`.
    fn events(&self) -> Vec<Value> {
        let sql = format!(
            "SELECT json_object('type', type, 'data', json(json_extract(json, '$.data')))
             FROM events WHERE stream = 'session/{}' ORDER BY seq",
            self.session
        );
        let rows = query(&self.data_dir.join("host.db"), &sql);
        rows.iter().map(|row| typed(row.parse().unwrap())).collect()
    }

    /// Exports the session's recording, its header and paths rewritten for
    /// `testdata/` (T116, T065); its path under `out`.
    async fn export(&self, scenario: &Scenario, dir: &TempDir, out: &Path) -> String {
        let params = typed(json!({"session_id": self.session}));
        let exported = self.client.request::<DiagRecordingExport>(params);
        let hash = exported.await.unwrap().blob.hash.to_string();
        let blobs = self.data_dir.join("blobs/sha256");
        let mut text = std::fs::read_to_string(blobs.join(&hash[..2]).join(&hash)).unwrap();
        let real = std::fs::canonicalize(dir.path()).unwrap();
        for project in [real.to_str().unwrap(), dir.path().to_str().unwrap()] {
            text = text.replace(project, WORK);
        }
        let mut recording = Recording::parse(&text).unwrap();
        let header = &mut recording.header;
        header.scenario = scenario.id.to_owned();
        let usage = if scenario.gold_b {
            Usage::GoldB
        } else {
            Usage::Replay
        };
        header.usage = vec![usage];
        header.adapter = "cupper-fake-agent".into();
        header.adapter_version = "0.0.0".into();
        header.cli_version = "fake".into();
        let name = format!("recordings/{}.jsonl", scenario.id);
        std::fs::write(out.join(&name), recording.to_jsonl()).unwrap();
        name
    }
}

/// The `data` of the events of type `kind`.
fn of<'a>(events: &'a [Value], kind: &'a str) -> impl Iterator<Item = &'a Value> {
    events
        .iter()
        .filter(move |e| e["type"] == kind)
        .map(|e| &e["data"])
}

/// A field's ID and answer: its first choice, or `text`.
fn answer(field: &Value, text: &str) -> (String, Value) {
    let first = field["choices"][0]["value"].clone();
    let value = match field["type"].as_str() {
        Some("single_choice") => first,
        Some("multi_choice") => json!([first]),
        Some("boolean") => json!(true),
        Some("number") => json!(1),
        _ => json!(text),
    };
    (field["field_id"].as_str().unwrap().to_owned(), value)
}

fn holds(check: &Check, events: &[Value], project: &Path) -> bool {
    let items = || of(events, "item.started");
    match check {
        Check::Turn(status) => of(events, "turn.completed")
            .last()
            .is_some_and(|d| d["status"] == *status),
        Check::Item(kind) => items().any(|d| d["kind"] == *kind),
        Check::Tool(kind) => items().any(|d| d["kind"] == "tool_call" && d["tool_kind"] == *kind),
        Check::ExitCode => ["item.started", "item.updated", "item.completed"]
            .iter()
            .flat_map(|kind| of(events, kind))
            .any(|d| d["exit_code"].as_i64().is_some_and(|code| code != 0)),
        Check::Request(kind, status) => of(events, "interaction.requested")
            .filter(|asked| asked["request"]["type"] == *kind)
            .any(|asked| {
                of(events, "interaction.resolved")
                    .any(|r| r["request_id"] == asked["request_id"] && r["status"] == *status)
            }),
        Check::Capabilities => of(events, "session.capabilities").any(|d| {
            d["config"]
                .as_array()
                .is_some_and(|config| !config.is_empty())
        }),
        Check::NoFile(path) => !project.join(path).exists(),
    }
}
