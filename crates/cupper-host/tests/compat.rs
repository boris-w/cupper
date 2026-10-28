//! The compat suite's runner (design 5.9.5), which `cargo xtask compat`
//! starts: an ignored test that reads its plan from `CUPPER_COMPAT_PLAN`,
//! plays each scenario on a host of its own, with the fake agent or the real
//! CLI, and writes what happened, with the session's recording, under the
//! plan's `out`. xtask makes the report from it. The real mode's
//! self-constraints are checked before and while a scenario plays.

mod common;
#[path = "compat/scenarios.rs"]
mod scenarios;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use cupper_cli::client::{self, Client};
use cupper_host::clock::{Clock, SystemClock};
use cupper_host::gateway::testkit::TempDir;
use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::methods::admin::DiagRecordingExport;
use cupper_protocol::methods::session::{SessionCreate, SessionInterrupt, SessionSend};
use cupper_protocol::recording::{Recording, Usage};
use serde::Deserialize;
use serde_json::{Value, json};

use common::*;
use scenarios::{Check, SCENARIOS, Scenario, Step};

/// What stands for the project's path in a recording.
const WORK: &str = "/work/project";
/// A 1×1 PNG, for the image scenario.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    /// The scenarios to play, by ID; every one when empty.
    only: Vec<String>,
    /// How many times a scenario is played while it stays inconclusive.
    attempts: u32,
    out: PathBuf,
    /// Print what would run and the self-constraints' verdicts; start nothing.
    dry_run: bool,
    /// The real CLI instead of the fake agent.
    real: Option<Real>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Real {
    /// The e2e build's `CupperHost.app`, which installs the runtime.
    host: PathBuf,
    model: String,
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "run by `cargo xtask compat`"]
async fn compat_driver() {
    let plan = std::env::var("CUPPER_COMPAT_PLAN").expect("CUPPER_COMPAT_PLAN: the plan's path");
    let plan: Plan = serde_json::from_str(&std::fs::read_to_string(plan).unwrap()).unwrap();
    for id in &plan.only {
        assert!(SCENARIOS.iter().any(|s| s.id == id), "no scenario {id}");
    }
    let chosen = |s: &&Scenario| plan.only.is_empty() || plan.only.iter().any(|id| id == s.id);
    if plan.dry_run {
        let temp = own_temp(TempDir::new("compat-probe").path());
        println!("temporary repositories in the user's own temporary directory: {temp}");
        for s in SCENARIOS.iter().filter(chosen) {
            let files: Vec<_> = s.files.iter().map(|(path, _)| *path).collect();
            let refused = s.refused("claude").unwrap_or_else(|| "nothing".into());
            let skip = s.skip.unwrap_or("no");
            println!(
                "  {:<16} mode {:<8} files {files:?}; skipped: {skip}; refused: {refused}",
                s.id, s.mode
            );
        }
        return;
    }
    std::fs::create_dir_all(plan.out.join("recordings")).unwrap();
    let clock = SystemClock::new();
    let (mut results, mut roots, mut sessions) = (Vec::new(), Vec::new(), Vec::new());
    for scenario in SCENARIOS.iter().filter(chosen) {
        let started = clock.now_mono();
        let mut attempts = 0;
        let mut played = loop {
            attempts += 1;
            let dir = TempDir::new(&format!("compat-{}", scenario.id));
            let real = std::fs::canonicalize(dir.path()).unwrap();
            roots.extend([dir.path().to_owned(), real]);
            let run = play(scenario, dir, plan.out.clone(), plan.real.clone());
            let played = match tokio::spawn(run).await {
                Ok(played) => played,
                Err(err) => ended("fail", &format!("the runner panicked: {}", panicked(err))),
            };
            if played["outcome"] != "inconclusive" || attempts >= plan.attempts {
                break played;
            }
        };
        let played_map = played.as_object_mut().unwrap();
        if let Some(Value::Array(ids)) = played_map.remove("cli_sessions") {
            sessions.extend(ids);
        }
        let ms = clock
            .now_mono()
            .saturating_duration_since(started)
            .as_millis();
        let more: serde_json::Map<_, _> = typed(json!({"id": scenario.id, "title": scenario.title,
            "gold_b": scenario.gold_b, "attempts": attempts, "duration_ms": ms as u64}));
        played_map.extend(more);
        results.push(played);
    }
    // A real run's versions are what the host saw, in a recording's header.
    let recorded = results.iter().find_map(|r| r["recording"].as_str());
    let header = match (&plan.real, recorded) {
        (Some(_), Some(path)) => std::fs::read_to_string(plan.out.join(path)).unwrap(),
        (Some(_), None) => r#"{"adapter":"unknown","adapter_version":"unknown","cli":"claude","cli_version":"unknown"}"#.into(),
        (None, _) => r#"{"adapter":"cupper-fake-agent","adapter_version":"0.0.0","cli":"claude","cli_version":"fake"}"#.into(),
    };
    let h: Value = header.lines().next().unwrap().parse().unwrap();
    let run = json!({"adapter": {"name": h["adapter"], "version": h["adapter_version"]},
        "cli": {"name": h["cli"], "version": h["cli_version"]}, "roots": roots,
        "cli_sessions": sessions, "scenarios": results});
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

/// Whether `path`, which exists, is inside the user's own temporary
/// directory, which no other user shares (design 5.9.5).
fn own_temp(path: &Path) -> bool {
    let own = cupper_sys::user_temp_dir()
        .ok()
        .and_then(|own| own.canonicalize().ok());
    let path = path.canonicalize().ok();
    own.zip(path)
        .is_some_and(|(own, path)| path.starts_with(&own) && path != own)
}

/// The host on `data_dir`. A real run starts it as the user would, to find
/// the user's CLI and its login; the fake one, as every other test (T203).
fn start_host(program: &Path, data_dir: &Path, config: &Value, real: bool) -> Child {
    let config = config.clone();
    match real {
        true => start(&mut Command::new(program), data_dir, &payload(config)),
        false => spawn_from(program, data_dir, config),
    }
}

async fn play(
    scenario: &'static Scenario,
    dir: TempDir,
    out: PathBuf,
    real: Option<Real>,
) -> Value {
    if let Some(why) = scenario.skip {
        return ended("not_applicable", why);
    }
    if let (Some(why), None) = (scenario.real_only, &real) {
        return ended("real_only", why);
    }
    if let Some(refused) = scenario.refused("claude") {
        return ended("fail", &format!("refused: {refused}"));
    }
    let repo = dir.path().join("repo");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::create_dir(&repo).unwrap();
    assert!(
        own_temp(&repo),
        "refused: not the user's own temporary directory"
    );
    for (path, text) in scenario.files {
        std::fs::create_dir_all(repo.join(path).parent().unwrap()).unwrap();
        std::fs::write(repo.join(path), text).unwrap();
    }
    let project = scenario.project.map_or(repo.clone(), |sub| repo.join(sub));
    let trust = if scenario.untrusted {
        "untrusted"
    } else {
        "trusted"
    };
    let (data_dir, conn) = migrated(&dir);
    let sql = "INSERT INTO projects (project_id, path, name, position, added_at, trust_state)
               VALUES (?1, ?2, 'p', 0, 1, ?3)";
    let row = [
        ProjectId::from_uuid(1).to_string(),
        project.display().to_string(),
        trust.into(),
    ];
    conn.execute(sql, row).unwrap();
    let (program, config) = match &real {
        // The login shell's environment, filtered by the host as ever.
        Some(real) => {
            let init = output(
                "/usr/bin/git",
                &["-C", repo.to_str().unwrap(), "init", "-q"],
            );
            assert!(init.status.success(), "git init");
            (real.host.clone(), json!({"notifier": "record"}))
        }
        None => (PathBuf::from(HOST), fake_agent_config(json!({}))),
    };
    let host = Running(start_host(&program, &data_dir, &config, real.is_some()));
    let client = connect_blobs(&data_dir).await;
    let model = real.as_ref().map(|r| scenario.model.unwrap_or(&r.model));
    let session = create(&client, scenario, real.is_some(), model).await;
    let patience = Duration::from_secs(if real.is_some() { 300 } else { 30 });
    let mut user = User {
        data_dir,
        session,
        client,
        asked: 0,
        loosens: scenario.loosens,
        host,
        repo: repo.clone(),
        program,
        config,
        real: real.is_some(),
        model: model.map(str::to_owned),
        scenario,
        patience,
        sent: 1,
        markers: Vec::new(),
        counts: Vec::new(),
    };
    if let Err(waited) = user.normal_mode(scenario.mode).await {
        return ended("inconclusive", &waited);
    }
    if let Err(waited) = user.play(scenario.steps).await {
        // The agent did not do what the scenario needs (design 5.9.5).
        return ended("inconclusive", &waited);
    }
    let recording = user.export(scenario, &project, &out, real.is_none()).await;
    let host = user.host.0.id();
    let procs = tree(host);
    let below = procs.iter().any(|(_, ppid)| *ppid != host);
    let text = std::fs::read_to_string(out.join(&recording)).unwrap();
    let check = |check: &str, passed: bool| json!({"check": check, "passed": passed});
    let judge = |c: &Check| (c.name(), user.judge(c, &repo, &text, below));
    let observe = |c: &&Check| matches!(c, Check::Observe(_));
    let (observed, judged): (Vec<_>, Vec<_>) = scenario.checks.iter().partition(observe);
    let mut checks: Vec<_> = judged
        .into_iter()
        .map(judge)
        .map(|(c, p)| check(&c, p))
        .collect();
    let seen: Vec<_> = observed
        .into_iter()
        .map(judge)
        .map(|(c, p)| format!("{c}: {p}"))
        .collect();
    let held = holding(host, &procs);
    checks.push(check(
        "no descendant of an adapter holds the host's pipes or sockets",
        held.is_empty(),
    ));
    terminate(&user.host.0);
    let code = exit_code(&mut user.host.0).await;
    let left = leftovers(&procs).await;
    checks.push(check("no process is left once the host exits", left == 0));
    let unmet = checks.iter().find(|check| check["passed"] == false);
    let detail = match (code, unmet) {
        (0, None) => None,
        (0, Some(check)) => Some(format!("not met: {}", check["check"].as_str().unwrap())),
        (code, _) => Some(format!("the host exited with {code}")),
    };
    let sessions = if real.is_some() {
        user.cli_sessions()
    } else {
        Vec::new()
    };
    // An observing scenario reports what it saw (Q28, Q34).
    let (outcome, detail) = match detail {
        Some(detail) => ("fail", Some(detail)),
        None if !seen.is_empty() => ("observed", Some(seen.join("; "))),
        None => ("pass", None),
    };
    json!({"outcome": outcome, "detail": detail,
        "checks": checks, "recording": recording, "cli_sessions": sessions})
}

/// The harness client, able to upload blobs (the image scenario).
async fn connect_blobs(data_dir: &Path) -> Client {
    use cupper_cli::client::{ClientAuth, Options};
    use cupper_cli::harness::HarnessKey;
    use cupper_host::gateway::testkit::KEY;
    use cupper_protocol::methods::handshake::Feature;
    let auth = ClientAuth::Harness(HarnessKey::new(KEY));
    let options = Options {
        auth,
        features: vec![Feature::BlobPut],
        ..Options::default()
    };
    connect_with(data_dir, options).await
}

/// `session/create` in the scenario's permission mode, always given.
async fn create(
    client: &Client,
    scenario: &Scenario,
    real: bool,
    model: Option<&str>,
) -> SessionId {
    let text = if real {
        scenario.prompt
    } else {
        scenario.script
    };
    let params = json!({"command_id": client::command_id(), "agent": "claude",
        "project_id": ProjectId::from_uuid(1), "workspace": {"kind": "local"},
        "config": {"permission_mode": scenario.mode, "model": model},
        "message": [{"type": "text", "text": text}]});
    let created = client.request::<SessionCreate>(typed(params));
    created.await.unwrap().session_id
}

struct User {
    data_dir: PathBuf,
    session: SessionId,
    client: Client,
    /// The requests a step has taken.
    asked: usize,
    /// The scenario may loosen the tier.
    loosens: bool,
    host: Running,
    repo: PathBuf,
    /// How the host starts, for a restart.
    program: PathBuf,
    config: Value,
    real: bool,
    model: Option<String>,
    scenario: &'static Scenario,
    /// How long a wait lasts: real turns take longer.
    patience: Duration,
    /// The prompts sent so far.
    sent: usize,
    /// `Concurrent`'s markers, by the session that asked for them.
    markers: Vec<(SessionId, String)>,
    /// `Resumes`' counts of the host's processes, the first before them.
    counts: Vec<usize>,
}

impl User {
    /// Refuses a mode that the host rates looser than `normal`, once the
    /// session's capabilities say how it rates them.
    async fn normal_mode(&self, mode: &str) -> Result<(), String> {
        let modes = self.wait("the session's capabilities", |events| {
            let config = of(events, "session.capabilities").next()?["config"]
                .as_array()?
                .clone();
            config
                .into_iter()
                .find(|option| option["id"] == "permission_mode")
        });
        let modes = modes.await?;
        let values = modes["values"].as_array().into_iter().flatten();
        let risk = values
            .into_iter()
            .find(|v| v["value"] == mode)
            .map(|v| v["risk"].clone());
        let normal = risk.as_ref().is_some_and(|risk| risk == "normal");
        assert!(
            normal || self.loosens,
            "refused: permission mode `{mode}` of risk {risk:?}"
        );
        Ok(())
    }

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
                    let privileged = option["requires"] == "privileged";
                    let persistent = option["effect"]["persistent"] == true;
                    let loosens = privileged || persistent;
                    assert!(
                        !loosens || self.loosens,
                        "refused: option {}",
                        option["option_id"]
                    );
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
                Step::Send { real, fake } => self.send_next(real, fake).await?,
                Step::Image { real, fake } => self.image(real, fake).await?,
                Step::KillCli => self.kill_cli().await?,
                Step::RestartHost => {
                    self.settle().await?;
                    terminate(&self.host.0);
                    assert_eq!(exit_code(&mut self.host.0).await, 0, "the host stopped");
                    let host = start_host(&self.program, &self.data_dir, &self.config, self.real);
                    self.host = Running(host);
                    self.client = connect_blobs(&self.data_dir).await;
                }
                Step::Resumes(times) => {
                    self.settle().await?;
                    self.counts.push(tree(self.host.0.id()).len());
                    for _ in 0..*times {
                        self.kill_cli().await?;
                        self.send_next("Reply with the single word: again", "crash_cli")
                            .await?;
                        self.send_next("", "whoami").await?;
                        self.settle().await?;
                        self.counts.push(tree(self.host.0.id()).len());
                    }
                }
                Step::Concurrent {
                    sessions,
                    prompts,
                    rounds,
                } => {
                    self.concurrent(*sessions, *prompts, *rounds).await?;
                }
                Step::WriteFile(path, text) => {
                    let path = self.repo.join(path);
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(path, text).unwrap();
                }
                Step::Leave => {}
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
        match steps.last() {
            Some(Step::Leave) => Ok(()),
            _ => self.settle().await,
        }
    }

    /// Until every prompt sent has had its turn.
    async fn settle(&self) -> Result<(), String> {
        let sent = self.sent;
        let ended = |events: &[Value]| (of(events, "turn.completed").count() >= sent).then_some(());
        self.wait("the end of the turn", ended).await
    }

    /// The next prompt, once the turn before has ended; none when this
    /// mode's text is empty.
    async fn send_next(&mut self, real: &str, fake: &str) -> Result<(), String> {
        self.settle().await?;
        let text = if self.real { real } else { fake };
        if !text.is_empty() {
            send(&self.client, &self.session, text).await;
            self.sent += 1;
        }
        Ok(())
    }

    /// Kills the real CLI below the adapter, after the turn; the fake
    /// agent's script crashes it instead.
    async fn kill_cli(&self) -> Result<(), String> {
        self.settle().await?;
        for (pid, _) in tree(self.host.0.id()).iter().filter(|_| self.real) {
            let name = output("/bin/ps", &["-o", "comm=", "-p", &pid.to_string()]);
            if String::from_utf8_lossy(&name.stdout)
                .trim()
                .ends_with("claude")
            {
                output("/bin/kill", &["-KILL", &pid.to_string()]);
            }
        }
        Ok(())
    }

    /// Prompts with a 1×1 PNG, uploaded first.
    async fn image(&mut self, real: &str, fake: &str) -> Result<(), String> {
        use base64::Engine as _;
        use cupper_protocol::frame::BlobChunkHeader;
        use cupper_protocol::methods::stream::BlobPut;
        use sha2::Digest as _;
        self.settle().await?;
        let png = base64::engine::general_purpose::STANDARD
            .decode(PNG)
            .unwrap();
        let raw: [u8; 32] = sha2::Sha256::digest(&png).into();
        let header = BlobChunkHeader {
            hash: raw,
            offset: 0,
            last: true,
        };
        self.client.send_blob_chunk(&header, &png).await.unwrap();
        let blob = json!({"hash": cupper_protocol::ids::BlobHash::from_sha256(&raw),
            "size": png.len(), "media_type": "image/png"});
        self.client
            .request::<BlobPut>(typed(blob.clone()))
            .await
            .unwrap();
        let text = if self.real { real } else { fake };
        let content = json!([{"type": "text", "text": text}, {"type": "image", "blob": blob}]);
        let params = json!({"command_id": client::command_id(), "session_id": self.session,
            "mode": "queue", "content": content});
        self.client
            .request::<SessionSend>(typed(params))
            .await
            .unwrap();
        self.sent += 1;
        Ok(())
    }

    /// `sessions` sessions, this one among them, each sent `prompts` at
    /// once, `rounds` times; every prompt asks for a marker of its own.
    async fn concurrent(
        &mut self,
        sessions: usize,
        prompts: usize,
        rounds: usize,
    ) -> Result<(), String> {
        self.settle().await?;
        let mut all = vec![self.session.clone()];
        for _ in 1..sessions {
            all.push(
                create(
                    &self.client,
                    self.scenario,
                    self.real,
                    self.model.as_deref(),
                )
                .await,
            );
        }
        let nonce = &client::command_id().to_string()[..8];
        for round in 1..=rounds {
            for (n, session) in all.iter().enumerate() {
                for prompt in 1..=prompts {
                    let marker = format!("cupper-{nonce}-{round}-{n}-{prompt}");
                    let text = if self.real {
                        format!("Reply with exactly this and nothing else: {marker}")
                    } else {
                        format!("say {marker}")
                    };
                    send(&self.client, session, &text).await;
                    self.markers.push((session.clone(), marker));
                }
            }
            let turns = 1 + round * prompts;
            let done = || {
                all.iter()
                    .all(|s| of(&self.events_of(s), "turn.completed").count() >= turns)
            };
            self.until("every session's turns", || done().then_some(()))
                .await?;
        }
        self.sent += rounds * prompts;
        Ok(())
    }

    /// The checks this runner's state answers; the rest from the events.
    fn judge(&self, check: &Check, repo: &Path, recording: &str, below: bool) -> bool {
        let events = self.events();
        let frames = || {
            recording
                .lines()
                .filter_map(|line| line.parse::<Value>().ok())
        };
        let sent = |method: &'static str| {
            frames().filter(move |f| f["dir"] == "send" && f["msg"]["method"] == method)
        };
        match check {
            Check::Lacks(path, text) => !std::fs::read_to_string(repo.join(path))
                .unwrap_or_default()
                .contains(text),
            Check::Notice(code) => {
                of(&events, "item.started").any(|d| d["kind"] == "notice" && d["code"] == *code)
            }
            Check::NoCrossTalk => self.markers.iter().all(|(session, marker)| {
                let said: String = of(&self.events_of(session), "item.delta")
                    .filter_map(|d| d["text"].as_str())
                    .collect();
                let others = self.markers.iter().filter(|(s, _)| s != session);
                said.contains(marker.as_str())
                    && !others
                        .clone()
                        .any(|(_, other)| said.contains(other.as_str()))
            }),
            Check::Steady => self.counts.iter().all(|count| *count <= self.counts[0]),
            Check::Running => below,
            Check::UserSettings => sent("session/new").any(|f| {
                f["msg"]["params"]["_meta"]["claudeCode"]["options"]["settingSources"]
                    == json!(["user"])
            }),
            Check::ImageSent => sent("session/prompt").any(|f| {
                let blocks = f["msg"]["params"]["prompt"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                blocks.iter().any(|block| block["type"] == "image")
            }),
            Check::Sent(method) => sent(method).next().is_some(),
            Check::Observe(path) => repo.join(path).exists(),
            _ => holds(check, &events, repo),
        }
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
        self.until(what, || find(&self.events())).await
    }

    async fn until<T>(&self, what: &str, find: impl Fn() -> Option<T>) -> Result<T, String> {
        let clock = SystemClock::new();
        let give_up = clock.now_mono() + self.patience;
        loop {
            if let Some(found) = find() {
                return Ok(found);
            }
            if clock.now_mono() >= give_up {
                return Err(format!("no {what} within {}s", self.patience.as_secs()));
            }
            wait(&clock).await;
        }
    }

    /// The session's events so far, as `{type, data}`.
    fn events(&self) -> Vec<Value> {
        self.events_of(&self.session)
    }

    fn events_of(&self, session: &SessionId) -> Vec<Value> {
        let sql = format!(
            "SELECT json_object('type', type, 'data', json(json_extract(json, '$.data')))
             FROM events WHERE stream = 'session/{session}' ORDER BY seq"
        );
        let rows = query(&self.data_dir.join("host.db"), &sql);
        rows.iter().map(|row| typed(row.parse().unwrap())).collect()
    }

    /// Exports the session's recording, its header and paths rewritten for
    /// `testdata/` (T116, T065); its path under `out`.
    async fn export(&self, scenario: &Scenario, repo: &Path, out: &Path, fake: bool) -> String {
        let params = typed(json!({"session_id": self.session}));
        let exported = self.client.request::<DiagRecordingExport>(params);
        let hash = exported.await.unwrap().blob.hash.to_string();
        let blobs = self.data_dir.join("blobs/sha256");
        let mut text = std::fs::read_to_string(blobs.join(&hash[..2]).join(&hash)).unwrap();
        let real = std::fs::canonicalize(repo).unwrap();
        for project in [real.to_str().unwrap(), repo.to_str().unwrap()] {
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
        if fake {
            header.adapter = "cupper-fake-agent".into();
            header.adapter_version = "0.0.0".into();
            header.cli_version = "fake".into();
        }
        let name = format!("recordings/{}.jsonl", scenario.id);
        std::fs::write(out.join(&name), recording.to_jsonl()).unwrap();
        name
    }

    /// The CLI's sessions this host started, to clean up after a real run.
    fn cli_sessions(&self) -> Vec<String> {
        let sql = "SELECT agent_session_id FROM sessions WHERE agent_session_id IS NOT NULL";
        query(&self.data_dir.join("host.db"), sql)
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
        _ => unreachable!("the runner's state answers {}", check.name()),
    }
}

/// `root`'s descendants as `(pid, parent)`, from `ps`.
fn tree(root: u32) -> Vec<(u32, u32)> {
    let ps = output("/bin/ps", &["-axo", "pid=,ppid="]);
    let all: Vec<(u32, u32)> = String::from_utf8_lossy(&ps.stdout)
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace().map(str::parse);
            Some((words.next()?.ok()?, words.next()?.ok()?))
        })
        .collect();
    let (mut found, mut parents) = (Vec::new(), vec![root]);
    while let Some(parent) = parents.pop() {
        let children = all.iter().filter(|(_, ppid)| *ppid == parent);
        children.for_each(|&(pid, ppid)| {
            found.push((pid, ppid));
            parents.push(pid);
        });
    }
    found
}

/// The processes below the host's children, the adapters, that hold one of
/// the host's pipe or socket ends, or their peers (security #19).
fn holding(host: u32, procs: &[(u32, u32)]) -> Vec<u32> {
    let below = procs.iter().filter(|(_, ppid)| *ppid != host);
    let below: Vec<u32> = below.map(|p| p.0).collect();
    if below.is_empty() {
        return below;
    }
    let host_ends = ends(&lsof(&[host]))
        .into_iter()
        .flat_map(|(_, own, peer)| [own, peer]);
    let host_ends: Vec<String> = host_ends.collect();
    let held = ends(&lsof(&below))
        .into_iter()
        .filter(|(_, own, peer)| host_ends.contains(own) || host_ends.contains(peer));
    held.map(|(pid, ..)| pid).collect()
}

fn lsof(pids: &[u32]) -> String {
    let pids: Vec<_> = pids.iter().map(u32::to_string).collect();
    let lsof = output(
        "/usr/sbin/lsof",
        &["-nP", "-F", "ptdn", "-p", &pids.join(",")],
    );
    String::from_utf8_lossy(&lsof.stdout).into_owned()
}

/// The pipe and Unix socket ends in `lsof -F ptdn` output: the process, the
/// end's address and the peer's.
fn ends(lsof: &str) -> Vec<(u32, String, String)> {
    let (mut found, mut pid, mut kind, mut own) = (Vec::new(), 0, String::new(), String::new());
    for line in lsof.lines().filter(|line| !line.is_empty()) {
        let (field, value) = line.split_at(1);
        match field {
            "p" => pid = value.parse().unwrap_or(0),
            "t" => kind = value.to_owned(),
            "d" => own = value.to_owned(),
            "n" if kind == "PIPE" || kind == "unix" => {
                let peer = value.strip_prefix("->").unwrap_or_default();
                found.push((pid, std::mem::take(&mut own), peer.to_owned()));
            }
            _ => {}
        }
    }
    found
}

/// How many of `procs` still run once the host has exited, after a grace;
/// those are stopped, as this run started them.
async fn leftovers(procs: &[(u32, u32)]) -> usize {
    let alive = |pid: &u32| {
        output("/bin/kill", &["-0", &pid.to_string()])
            .status
            .success()
    };
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + Duration::from_secs(5);
    let mut left: Vec<u32> = procs.iter().map(|p| p.0).collect();
    while !left.is_empty() && clock.now_mono() < give_up {
        left.retain(alive);
        wait(&clock).await;
    }
    for pid in &left {
        output("/bin/kill", &["-TERM", &pid.to_string()]);
    }
    left.len()
}

/// What `program` printed; it is one of the system's tools.
fn output(program: &str, args: &[&str]) -> std::process::Output {
    let mut command = Command::new(program);
    command
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_compat_guards_every_scenario_starts_in_an_allowed_mode_and_picks_once() {
        for scenario in SCENARIOS {
            assert_eq!(scenario.refused("claude"), None, "{}", scenario.id);
        }
        let base = || Scenario {
            mode: "default",
            ..SCENARIOS[0]
        };
        let refused = |s: Scenario, agent| s.refused(agent).is_some();
        assert!(!refused(
            Scenario {
                mode: "workspace-write",
                ..base()
            },
            "codex"
        ));
        assert!(refused(
            Scenario {
                mode: "acceptEdits",
                ..base()
            },
            "claude"
        ));
        assert!(refused(
            Scenario {
                mode: "danger-full-access",
                ..base()
            },
            "codex"
        ));
        assert!(refused(
            Scenario {
                mode: "",
                loosens: true,
                ..base()
            },
            "claude"
        ));
        assert!(!refused(
            Scenario {
                mode: "acceptEdits",
                loosens: true,
                ..base()
            },
            "claude"
        ));
        let always = Scenario {
            steps: &[Step::Choose("allow_always")],
            loosens: true,
            ..base()
        };
        assert!(refused(always, "claude"));
    }

    #[test]
    fn unit_compat_guards_repositories_stay_in_the_users_own_temporary_directory() {
        let dir = TempDir::new("compat-own");
        assert!(own_temp(dir.path()));
        assert!(!own_temp(Path::new("/tmp")));
        assert!(!own_temp(&cupper_sys::user_temp_dir().unwrap()));
        assert!(!own_temp(&dir.path().join("missing")));
    }

    #[test]
    fn unit_compat_guards_lsof_names_each_pipe_and_socket_end() {
        let text = "p10\nf0\ntPIPE\nd0xa1\nn->0xb2\nf3\ntREG\nn/x\np11\nf4\ntunix\nd0xc3\nn/sock\n";
        let pipe = (10, "0xa1".to_owned(), "0xb2".to_owned());
        assert_eq!(ends(text), [pipe, (11, "0xc3".to_owned(), String::new())]);
    }
}
