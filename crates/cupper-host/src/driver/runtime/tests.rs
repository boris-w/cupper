//! `AgentRuntime` with the fake agent (design 5.6.1, 5.6.6, 5.6.8, 5.8.5):
//! IT-process-hygiene, IT-K13-echo-cwd, IT-K26-client-info, IT-K21-reap-group,
//! IT-adapter-restart, and IT-orch-gating's `SetConfig`. The runtime's timers
//! run on `ManualClock`, which [`Fixture::tick`] moves to the deadlines the
//! runtime sets.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::clock::{Clock, ManualClock, MonoTime, SystemClock};
use crate::diag::full::Redactor;
use crate::diag::stderr::AdapterLog;
use crate::driver::api::{NormUpdate, PromptTag};
use crate::driver::probe::login_shell::{Failure, LoginEnv};
use crate::orchestrator::testkit::text;
use crate::store::testutil::{TempDir, host_writer, query, raw_db, raw_rows};
use crate::store::{Durability, RAW_WRITER, RawWriter, RawWriterThread, WriterThread};
use cupper_protocol::index::AgentStatus;
use cupper_sys::{ProcessGroup, ProcessInfo, process_info};

type Events = mpsc::UnboundedReceiver<DriverEvent>;

struct Fixture {
    dir: TempDir,
    clock: ManualClock,
    runtime: AgentRuntime,
    raw: Option<RawWriterThread>,
    _writer: WriterThread,
}

impl Fixture {
    /// The fake agent with `args`; `cli` is the CLI whose processes count as
    /// `claude`.
    fn new(name: &str, args: &[&str], cli: Option<&str>, rss: Option<u64>) -> Self {
        let args = args.iter().map(|arg| (*arg).to_owned()).collect();
        let mut launch = launch::injected(launch::fake_agent(), args, &host(), &login());
        launch.cli = cli.map(|cli| launch::Cli {
            path: cli.into(),
            version: String::new(),
            install: cupper_protocol::index::CliInstall::Native,
        });
        Self::with(name, launch, rss)
    }

    fn with(name: &str, launch: Launch, rss: Option<u64>) -> Self {
        let (dir, clock) = (TempDir::new(name), ManualClock::new());
        let (writer, _writer) = host_writer(&dir, &clock);
        let quiescence = Quiescence::new();
        quiescence.register(RAW_WRITER);
        quiescence.register(AGENT_RUNTIME);
        let (raw, raw_thread) = RawWriter::spawn(raw_db(&dir), &quiescence).unwrap();
        let shared: Arc<dyn Clock> = Arc::new(clock.clone());
        let blobs = BlobStore::new(dir.path(), writer.clone(), Arc::clone(&shared));
        let log = AdapterLog::unwritten(Arc::clone(&shared), Redactor::default());
        let raw = raw.sink();
        let cx = Context {
            clock: Arc::clone(&shared),
            writer,
            raw,
            log,
            self_check: None,
        };
        let (ids, durations) = (Arc::new(IdGen::seeded(shared, 1)), Durations::default());
        let runtime = AgentRuntime::start(RuntimeDeps {
            launch,
            cx,
            ids,
            blobs,
            quiescence,
            probe_dir: dir.path().join("probe"),
            durations,
            notes: Arc::new(Deaf),
            rss,
        });
        let raw = Some(raw_thread);
        Self {
            dir,
            clock,
            runtime,
            raw,
            _writer,
        }
    }

    /// Runs `future`, moving the clock to each deadline the runtime sets
    /// meanwhile up to `secs` from now; later ones, such as the next sample,
    /// stay ahead.
    async fn tick<T>(&self, secs: u64, future: impl Future<Output = T>) -> T {
        let (mut future, limit) = (
            pin!(future),
            self.clock.now_mono() + Duration::from_secs(secs),
        );
        loop {
            tokio::select! {
                out = &mut future => return out,
                () = tokio::task::yield_now() => {
                    if let Some(at) = self.clock.next_deadline().filter(|at| *at <= limit) {
                        self.clock.advance_to(at);
                    }
                }
            }
        }
    }

    /// A new session in the fixture's directory, and its events.
    async fn attach(&self) -> Result<(AgentSessionId, Events), AgentError> {
        let (events, heard) = mpsc::unbounded_channel();
        let (target, cwd) = (AttachTarget::New, self.dir.path().to_owned());
        let (config, project_config) = (BTreeMap::new(), ProjectConfigPolicy::Load);
        let attach = Attach {
            target,
            cwd,
            config,
            project_config,
        };
        let info = self.tick(8, self.runtime.attach(attach, events)).await?;
        Ok((info.session, heard))
    }

    /// Runs `script` as a prompt in `session`; what the agent said.
    async fn say(&self, session: &mut (AgentSessionId, Events), script: &str) -> String {
        let heard = self.prompt(session, script);
        let mut said = String::new();
        loop {
            match heard.recv().await.unwrap() {
                DriverEvent::Update(NormUpdate::AgentText { text, .. }) => said.push_str(&text),
                DriverEvent::PromptDone { .. } => break said,
                DriverEvent::PromptFailed { error, .. } => panic!("{error:?}"),
                _ => {}
            }
        }
    }

    /// The adapter exits; once its sessions heard it.
    async fn crash(&self, session: &mut (AgentSessionId, Events)) {
        let heard = self.prompt(session, "exit 1");
        while !matches!(heard.recv().await, Some(DriverEvent::AdapterExited { .. })) {}
    }

    fn prompt<'a>(
        &self,
        (session, heard): &'a mut (AgentSessionId, Events),
        script: &str,
    ) -> &'a mut Events {
        let (session, tag, content) = (session.clone(), PromptTag(1), text(script));
        self.runtime.send(DriverCommand::Prompt {
            session,
            tag,
            content,
        });
        heard
    }

    /// The latest adapter process, while it runs.
    fn adapter(&self) -> ProcessInfo {
        let sql = "SELECT pid FROM adapters ORDER BY started_at DESC, rowid DESC";
        let pid: Vec<i32> = query(&self.dir.host_db(), sql, |row| row.get(0));
        process_info(pid[0]).unwrap()
    }

    fn column(&self, sql: &str) -> Vec<String> {
        query(&self.dir.host_db(), sql, |row| row.get(0))
    }

    /// Stops the runtime; how long that took on the clock: 3 s for the
    /// sessions to close if they did not, 3 s after stdin closed, 2 s after
    /// SIGTERM at most.
    async fn stop(&self) -> Duration {
        let start = self.clock.now_mono();
        self.tick(8, self.runtime.stop(false)).await;
        self.clock.now_mono().saturating_duration_since(start)
    }
}

fn host() -> BTreeMap<String, String> {
    let var = |(k, v): (&str, &str)| (k.to_owned(), v.to_owned());
    BTreeMap::from([("HOME", "/h"), ("SECRET", "x"), ("TERM", "dumb")].map(var))
}

fn login() -> LoginEnv {
    LoginEnv::fallback(Failure::ShellMissing)
}

/// The pid an agent said as `pid=<pid>`, and that process.
fn said_pid(said: &str) -> ProcessInfo {
    let pid = said.rsplit("pid=").next().unwrap().parse().unwrap();
    process_info(pid).unwrap()
}

/// The process groups of an adapter's tree, recorded while it runs. Dropped,
/// whether the test passed or failed, it kills those still alive: the same
/// group id and start time, so nothing else. A test process killed outright
/// leaves its record in `cupper-test-groups` under the temp dir, and the
/// next test that records groups kills what is left of it.
struct Groups {
    groups: Vec<ProcessGroup>,
    record: PathBuf,
}

impl Groups {
    fn of(adapter: &ProcessInfo) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join("cupper-test-groups");
        fs::create_dir_all(&dir).unwrap();
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let pid = name.split('-').next().and_then(|pid| pid.parse().ok());
            if pid.is_some_and(|pid| process_info(pid).is_none()) {
                let left = fs::read_to_string(entry.path()).unwrap_or_default();
                kill(&left.lines().filter_map(group).collect::<Vec<_>>());
                let _ = fs::remove_file(entry.path());
            }
        }
        let leader = ProcessGroup {
            pgid: adapter.pgid,
            start: adapter.start,
        };
        let groups = leader.with_descendants();
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let record = dir.join(format!("{}-{n}", std::process::id()));
        let lines = groups
            .iter()
            .map(|g| format!("{} {}\n", g.pgid, g.start.as_micros()));
        fs::write(&record, lines.collect::<String>()).unwrap();
        Self { groups, record }
    }

    /// Whether every group is gone, given 5 s for the signals to land.
    async fn gone(&self) -> bool {
        let clock = SystemClock::new();
        let give_up = clock.now_mono() + Duration::from_secs(5);
        while self.groups.iter().any(ProcessGroup::is_alive) {
            if clock.now_mono() >= give_up {
                return false;
            }
            clock
                .timer_at(clock.now_mono() + Duration::from_millis(10))
                .await;
        }
        true
    }
}

impl Drop for Groups {
    fn drop(&mut self) {
        kill(&self.groups);
        let _ = fs::remove_file(&self.record);
    }
}

/// `<pgid> <start in µs>`, as `Groups` records a group.
fn group(line: &str) -> Option<ProcessGroup> {
    let (pgid, start) = line.split_once(' ')?;
    let start = cupper_sys::StartTime::from_micros(start.parse().ok()?);
    let pgid = pgid.parse().ok()?;
    Some(ProcessGroup { pgid, start })
}

fn kill(groups: &[ProcessGroup]) {
    for group in groups {
        let _ = group.signal(cupper_sys::Signal::Kill);
    }
}

/// An index that hears nothing.
#[derive(Debug)]
struct Deaf;

impl AgentNotes for Deaf {
    fn agent(&self, _: AgentStatus) {}
}

/// A runtime of the fake agent, on `dir`'s stores; probes in `dir/probe`.
fn runtime(dir: &TempDir) -> AgentRuntime {
    let clock = ManualClock::new();
    let (writer, _writer) = host_writer(dir, &clock);
    let quiescence = Quiescence::new();
    quiescence.register(RAW_WRITER);
    quiescence.register(AGENT_RUNTIME);
    let (raw, _raw) = RawWriter::spawn(raw_db(dir), &quiescence).unwrap();
    let clock: Arc<dyn Clock> = Arc::new(clock);
    let var = |(k, v): (&str, &str)| (k.to_owned(), v.to_owned());
    let host = BTreeMap::from([("HOME", "/h"), ("SECRET", "x"), ("TERM", "dumb")].map(var));
    let login = LoginEnv::fallback(Failure::ShellMissing);
    let launch = launch::injected(launch::fake_agent(), Vec::new(), &host, &login);
    let (raw, log) = (
        raw.sink(),
        AdapterLog::unwritten(Arc::clone(&clock), Redactor::default()),
    );
    let blobs = BlobStore::new(dir.path(), writer.clone(), Arc::clone(&clock));
    let cx = Context {
        clock: Arc::clone(&clock),
        writer,
        raw,
        log,
        self_check: None,
    };
    let ids = Arc::new(IdGen::seeded(clock, 1));
    AgentRuntime::start(RuntimeDeps {
        launch,
        cx,
        ids,
        blobs,
        quiescence,
        probe_dir: dir.path().join("probe"),
        durations: Durations::default(),
        notes: Arc::new(Deaf),
        rss: None,
    })
}

/// A new session in `cwd`, and what it hears.
async fn attach(
    runtime: &AgentRuntime,
    cwd: &Path,
) -> (AttachInfo, mpsc::UnboundedReceiver<DriverEvent>) {
    let (events, heard) = mpsc::unbounded_channel();
    let (target, cwd, config) = (AttachTarget::New, cwd.to_owned(), BTreeMap::new());
    let project_config = ProjectConfigPolicy::Load;
    let attach = Attach {
        target,
        cwd,
        config,
        project_config,
    };
    (runtime.attach(attach, events).await.unwrap(), heard)
}

/// Runs `script` as prompt `tag`; what the session heard until it was done.
async fn prompt(
    runtime: &AgentRuntime,
    session: &AgentSessionId,
    (tag, script): (u64, &str),
    heard: &mut mpsc::UnboundedReceiver<DriverEvent>,
) -> Vec<DriverEvent> {
    let (session, tag, content) = (session.clone(), PromptTag(tag), text(script));
    runtime.send(DriverCommand::Prompt {
        session,
        tag,
        content,
    });
    let mut events = Vec::new();
    loop {
        match heard.recv().await.unwrap() {
            DriverEvent::PromptDone { .. } => return events,
            DriverEvent::PromptFailed { error, .. } => panic!("{error:?}"),
            event => events.push(event),
        }
    }
}

/// Plays an in-process adapter: answers `initialize` and `session/new`,
/// nothing else, and exits once `release` fires, stdin closed or not.
#[cfg(feature = "test-harness")]
fn player(
    mut started: mpsc::UnboundedReceiver<launch::InProcessAgent>,
    release: oneshot::Receiver<()>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    tokio::spawn(async move {
        let agent = started.recv().await.unwrap();
        let (read, mut write) = tokio::io::split(agent.stream);
        let mut lines = BufReader::new(read).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let msg: serde_json::Value = serde_json::from_str(&line).unwrap();
            let result = match msg["method"].as_str() {
                Some("initialize") => json!({"protocolVersion": 1, "agentCapabilities": {}}),
                Some("session/new") => json!({"sessionId": "s"}),
                _ => continue,
            };
            let reply = json!({"jsonrpc": "2.0", "id": msg["id"], "result": result});
            write
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
        }
        let _ = release.await;
        drop(agent.exit);
    });
}

/// GOLD-B's in-process adapter (T065) has no process: `launch::start` gives
/// it no process group and writes no `adapters` row, so neither a stop nor
/// the next start's cleanup has any group, ours or 0, 1, -1, to signal. A
/// stop that waits for it walks every deadline, SIGTERM's and SIGKILL's
/// included, with nothing to send.
#[cfg(feature = "test-harness")]
#[tokio::test(flavor = "multi_thread")]
async fn gold_b_fake_in_process_adapter_has_no_process_to_signal() {
    let (agents, started) = mpsc::unbounded_channel();
    let mut launch = launch::injected(PathBuf::new(), Vec::new(), &host(), &login());
    launch.program = launch::Program::InProcess(launch::InProcess(agents));

    let (dir, clock) = (TempDir::new("in-process-launch"), ManualClock::new());
    let (writer, _writer) = host_writer(&dir, &clock);
    let quiescence = Quiescence::new();
    quiescence.register(RAW_WRITER);
    let (raw, _raw) = RawWriter::spawn(raw_db(&dir), &quiescence).unwrap();
    let clock: Arc<dyn Clock> = Arc::new(clock);
    let log = AdapterLog::unwritten(Arc::clone(&clock), Redactor::default());
    let (raw, self_check) = (raw.sink(), None);
    let cx = Context {
        clock,
        writer,
        raw,
        log,
        self_check,
    };
    let (_release, release) = oneshot::channel();
    player(started, release);
    let (mailbox, _heard) = mpsc::unbounded_channel::<FromAgent>();
    let id = AdapterInstanceId::from_uuid(1);
    let instance = launch::start(&launch, &cx, id, mailbox, |_| ())
        .await
        .unwrap();
    assert!(instance.group.is_none());

    let (agents, started) = mpsc::unbounded_channel();
    launch.program = launch::Program::InProcess(launch::InProcess(agents));
    let f = Fixture::with("in-process", launch, None);
    let (release, released) = oneshot::channel();
    player(started, released);
    f.attach().await.unwrap();
    assert_eq!(
        f.column("SELECT instance_id FROM adapters"),
        Vec::<String>::new()
    );
    // 3 s for the session to close, 3 s after stdin closed, 2 s after the
    // SIGTERM it does not get; then it exits.
    let killed = f.clock.now_mono() + Duration::from_secs(8);
    let exit = async {
        while f.clock.now_mono() < killed {
            tokio::task::yield_now().await;
        }
        release.send(()).unwrap();
    };
    let (took, ()) = tokio::join!(f.stop(), exit);
    assert_eq!(took, Duration::from_secs(8));
}

#[tokio::test(flavor = "multi_thread")]
async fn it_process_hygiene_no_inherited_fds_allowed_env_own_session() {
    let f = Fixture::new("hygiene", &[], None, None);
    let said = f.say(&mut f.attach().await.unwrap(), "probe").await;
    assert_eq!(said, "fds=0,1,2 env=CLAUDE_CODE_EXECUTABLE,HOME,PATH,TERM");
    // The leader of a session and a process group of its own.
    let process = f.adapter();
    assert_eq!([process.sid, process.pgid], [process.pid; 2]);
    f.stop().await;
    let ended = f.column("SELECT instance_id FROM adapters WHERE ended_at IS NOT NULL");
    assert_eq!(ended.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k13_echo_cwd_the_agent_gets_the_real_path() {
    let f = Fixture::new("k13", &[], None, None);
    let real = fs::canonicalize(f.dir.path()).unwrap();
    // `TMPDIR` is under `/var`, a link to `/private/var`.
    assert_ne!(real, f.dir.path());
    let said = f.say(&mut f.attach().await.unwrap(), "echo_env").await;
    let cwd = format!("cwd={} ", real.display());
    assert!(said.starts_with(&cwd), "{said}");
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k26_client_info_names_cupper_only() {
    let f = Fixture::new("k26", &[], None, None);
    let said = f.say(&mut f.attach().await.unwrap(), "echo_env").await;
    let (_, rest) = said.split_once(" clientInfo=").unwrap();
    let client: serde_json::Value =
        serde_json::from_str(rest.split_once(" env=").unwrap().0).unwrap();
    assert_eq!(client["name"], "cupper");
    f.stop().await;
}

/// T044's mode set back, on the agent: the session hears all the options.
#[tokio::test(flavor = "multi_thread")]
async fn it_orch_gating_set_config_comes_back_as_all_options() {
    let f = Fixture::new("set-config", &[], None, None);
    let (session, mut heard) = f.attach().await.unwrap();
    let (option, value) = ("permission_mode".to_owned(), "plan".to_owned());
    f.runtime.send(DriverCommand::SetConfig {
        session,
        option,
        value,
    });
    let options = loop {
        if let Some(DriverEvent::ConfigChanged(options)) = heard.recv().await {
            break options;
        }
    };
    let mode = options.iter().find(|o| o.id.as_str() == "permission_mode");
    assert_eq!((mode.unwrap().current.as_str(), options.len()), ("plan", 3));
    f.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k21_reap_group_a_stop_reaps_descendants_in_groups_of_their_own() {
    let f = Fixture::new("k21-stop", &[], None, None);
    let mut session = f.attach().await.unwrap();
    let said = f.say(&mut session, "spawn_child; spawn_group_child").await;
    let child = said_pid(&said);
    assert_eq!(child.pgid, child.pid);
    let tree = Groups::of(&f.adapter());
    assert_eq!(tree.groups.len(), 2);
    f.stop().await;
    assert!(tree.gone().await);
}

/// Here the adapter leaves before any signal: the clock stops short of
/// SIGTERM's turn, 3 s after stdin closed. The groups found when the stop
/// began get the signals all the same.
#[tokio::test(flavor = "multi_thread")]
async fn it_k21_reap_group_an_adapter_that_leaves_first_still_loses_its_descendants() {
    let f = Fixture::new("k21-left", &[], None, None);
    let mut session = f.attach().await.unwrap();
    f.say(&mut session, "spawn_child; spawn_group_child").await;
    let tree = Groups::of(&f.adapter());
    f.tick(2, f.runtime.stop(false)).await;
    assert!(tree.gone().await);
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k21_reap_group_the_closing_order_ends_with_sigkill() {
    let mut f = Fixture::new("k21-order", &["--fault", "linger"], None, None);
    f.attach().await.unwrap();
    let adapter = f.adapter();
    // Stdin closed: 3 s; SIGTERM ignored: 2 s; SIGKILL.
    let took = f.stop().await;
    assert!((5..=8).contains(&took.as_secs()), "{took:?}");
    assert_eq!(process_info(adapter.pid), None);
    let ended = f.column("SELECT exit_status FROM adapters");
    assert_eq!(ended, ["signal: 9 (SIGKILL)"]);
    f.raw.take().unwrap().stop().unwrap();
    let frames: Vec<_> = raw_rows(&f.dir).into_iter().map(|row| row.frame).collect();
    assert!(
        frames
            .iter()
            .any(|frame| frame.contains(r#""method":"session/close""#))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k21_reap_group_sampling_finds_the_groups_a_crash_leaves() {
    let f = Fixture::new("k21-sample", &[], Some("/bin/sleep"), Some(1 << 30));
    let mut session = f.attach().await.unwrap();
    let child = said_pid(&f.say(&mut session, "spawn_group_child").await);
    let tree = Groups::of(&f.adapter());
    // The first sample, 30 s after the start.
    let mut resources = f.runtime.resources();
    let sampled = (*f
        .tick(30, resources.wait_for(Option::is_some))
        .await
        .unwrap())
    .unwrap();
    // The child runs `/bin/sleep`, the CLI here.
    assert_eq!((sampled.rss_bytes, sampled.claude_processes), (1 << 30, 1));
    // The adapter dies; the group it left is killed, as sampling found it.
    f.crash(&mut session).await;
    assert!(tree.gone().await);
    let groups = f.column("SELECT groups FROM adapters");
    assert_eq!(
        groups,
        [format!("[[{},{}]]", child.pid, child.start.as_micros())]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn it_adapter_restart_backs_off_then_crashed() {
    let since = |f: &Fixture, start: MonoTime| f.clock.now_mono().saturating_duration_since(start);
    // Restarts wait 1 s, then 2 s.
    let f = Fixture::new("restart", &[], None, None);
    let mut session = f.attach().await.unwrap();
    for wait in [1, 2] {
        f.crash(&mut session).await;
        let start = f.clock.now_mono();
        session = f.attach().await.unwrap();
        assert_eq!(since(&f, start), Duration::from_secs(wait));
        assert_eq!(f.say(&mut session, "say up").await, "up");
    }
    f.stop().await;

    // Five failed starts within 5 minutes: `crashed`, until cleared.
    let f = Fixture::new("crashed", &["--fault", "exit_on_start"], None, None);
    let start = f.clock.now_mono();
    for _ in 0..5 {
        let failed = f.attach().await.unwrap_err();
        assert_eq!(failed.class, AgentErrorClass::RuntimeFailed);
    }
    assert_eq!(since(&f, start), Duration::from_secs(1 + 2 + 4 + 8));
    assert_eq!(
        f.attach().await.unwrap_err().class,
        AgentErrorClass::Crashed
    );
    f.runtime.clear_crashed();
    assert_eq!(
        f.attach().await.unwrap_err().class,
        AgentErrorClass::RuntimeFailed
    );
}

/// The fake agent's command list comes once to the session, after the
/// attach or with it; each turn's start repeats it without an event. A
/// different list comes, and so does the old one when it returns; the
/// capability cache follows.
#[tokio::test(flavor = "multi_thread")]
async fn unit_commands_dedup_the_same_list_is_not_sent_again() {
    let dir = TempDir::new("dedup");
    // A row of another adapter version, which the runtime drops as it starts.
    let (writer, thread) = host_writer(&dir, &ManualClock::new());
    let stale = |conn: &rusqlite::Connection| {
        let sql = "INSERT INTO agent_capabilities (agent, adapter_version, captured_at, json)
                   VALUES ('claude', 'old', 0, '{}')";
        conn.execute(sql, []).map(drop)
    };
    writer.submit(Durability::Normal, stale).await.unwrap();
    drop((writer, thread));
    let runtime = runtime(&dir);
    let (info, mut heard) = attach(&runtime, dir.path()).await;
    let changed = |events: Vec<DriverEvent>| -> Vec<usize> {
        let lists = events.into_iter().filter_map(|event| match event {
            DriverEvent::CommandsChanged(commands) => Some(commands.len()),
            _ => None,
        });
        lists.collect()
    };
    let persona = 5;
    let first = changed(prompt(&runtime, &info.session, (1, "say a"), &mut heard).await);
    let attached = info.capabilities.commands.len();
    assert!(
        (attached, first.as_slice()) == (persona, &[])
            || (attached, first.as_slice()) == (0, &[persona]),
        "{attached} {first:?}"
    );
    assert_eq!(
        changed(prompt(&runtime, &info.session, (2, "say b"), &mut heard).await),
        [] as [usize; 0]
    );
    let other = r#"raw {"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"${session}","update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"x","description":"y"}]}}}"#;
    assert_eq!(
        changed(prompt(&runtime, &info.session, (3, other), &mut heard).await),
        [1]
    );
    assert_eq!(
        changed(prompt(&runtime, &info.session, (4, "say c"), &mut heard).await),
        [persona]
    );
    runtime.stop(true).await;
    let rows: Vec<(String, String)> = query(
        &dir.host_db(),
        "SELECT adapter_version, json FROM agent_capabilities",
        |row| Ok((row.get(0)?, row.get(1)?)),
    );
    let [(version, json)] = &rows[..] else {
        panic!("{rows:?}")
    };
    let cached: SessionCapabilities = serde_json::from_str(json).unwrap();
    assert_eq!(
        (version, cached.commands.len()),
        (&cached.versions.adapter, persona)
    );
}

/// A probe empties its directory (0700), opens a session there, reads its
/// options and command list, and fills the cache; `agents/capabilities`
/// reads it, and `agents/refresh` relaunches and reports the agent.
#[tokio::test(flavor = "multi_thread")]
async fn agents_probe_cache_and_refresh() {
    use std::os::unix::fs::PermissionsExt;

    use self::agents::Agents;
    use crate::store::Readers;
    use cupper_protocol::index::{AgentState, AuthState, RuntimeState};

    let dir = TempDir::new("probe");
    let probe = dir.path().join("probe");
    fs::create_dir_all(&probe).unwrap();
    fs::write(probe.join("left-over"), "x").unwrap();
    let runtime = runtime(&dir);
    let (writer, _thread) = host_writer(&dir, &ManualClock::new());
    let readers = Readers::open(&dir.host_db()).unwrap();
    let find: agents::Find = Arc::new(|| {
        let login = LoginEnv::fallback(Failure::ShellMissing);
        let launch = launch::injected(launch::fake_agent(), Vec::new(), &BTreeMap::new(), &login);
        Box::pin(async move { launch })
    });
    let agents = Agents::new(runtime.clone(), readers, writer, find);
    let claude = AgentKind::Claude;
    assert_eq!(agents.capabilities(&claude).await.unwrap().cached, None);
    let codex = agents.capabilities(&AgentKind::Codex).await.unwrap_err();
    assert_eq!(
        codex.app_code(),
        Some(&cupper_protocol::error::ErrorCode::Unavailable)
    );

    let refreshed = agents.refresh(&claude).await.unwrap().status;
    assert_eq!(fs::read_dir(&probe).unwrap().count(), 0);
    assert_eq!(
        fs::metadata(&probe).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let cached = agents.capabilities(&claude).await.unwrap().cached.unwrap();
    assert_eq!(cached.capabilities.commands.len(), 5);
    assert!(cached.capabilities.turn.steer.supported);
    assert_eq!(
        (
            refreshed.state,
            refreshed.auth.state,
            refreshed.adapter.runtime
        ),
        (
            AgentState::Running,
            AuthState::LoggedIn,
            RuntimeState::Ready
        )
    );
    assert!(refreshed.cli.is_none());
    runtime.stop(true).await;
}
