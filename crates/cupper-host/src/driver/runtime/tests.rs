//! IT-process-hygiene, IT-K13-echo-cwd, IT-K26-client-info: `AgentRuntime`
//! with the fake agent (design 5.6.1, 5.6.6, 5.8.5).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::clock::{Clock, ManualClock};
use crate::driver::api::{NormUpdate, PromptTag};
use crate::driver::probe::login_shell::{Failure, LoginEnv};
use crate::orchestrator::testkit::text;
use crate::store::testutil::{TempDir, host_writer, query, raw_db};
use crate::store::{RAW_WRITER, RawWriter};
use cupper_sys::ProcessInfo;

/// Runs `script` as the first prompt of a new session in `cwd`, then stops
/// the runtime; what the agent said, and its process as it ran.
async fn run(dir: &TempDir, cwd: &Path, script: &str) -> (String, ProcessInfo) {
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
    let (raw, log) = (raw.sink(), None);
    let blobs = BlobStore::new(dir.path(), writer.clone(), Arc::clone(&clock));
    let cx = Context {
        clock: Arc::clone(&clock),
        writer,
        raw,
        log,
    };
    let ids = Arc::new(IdGen::seeded(clock, 1));
    let runtime = AgentRuntime::start(RuntimeDeps {
        launch,
        cx,
        ids,
        blobs,
        quiescence,
    });
    let (events, mut heard) = mpsc::unbounded_channel();
    let (target, cwd, config) = (AttachTarget::New, cwd.to_owned(), BTreeMap::new());
    let project_config = ProjectConfigPolicy::Load;
    let attach = Attach {
        target,
        cwd,
        config,
        project_config,
    };
    let session = runtime.attach(attach, events).await.unwrap().session;
    let (tag, content) = (PromptTag(1), text(script));
    runtime.send(DriverCommand::Prompt {
        session,
        tag,
        content,
    });
    let mut said = String::new();
    loop {
        match heard.recv().await.unwrap() {
            DriverEvent::Update(NormUpdate::AgentText { text, .. }) => said.push_str(&text),
            DriverEvent::PromptDone { .. } => break,
            DriverEvent::PromptFailed { error, .. } => panic!("{error:?}"),
            _ => {}
        }
    }
    let pid = query(&dir.host_db(), "SELECT pid FROM adapters", |row| row.get(0));
    let process = cupper_sys::process_info(pid[0]).unwrap();
    runtime.stop(true).await;
    (said, process)
}

#[tokio::test(flavor = "multi_thread")]
async fn it_process_hygiene_no_inherited_fds_allowed_env_own_session() {
    let dir = TempDir::new("hygiene");
    let (said, process) = run(&dir, dir.path(), "probe").await;
    assert_eq!(said, "fds=0,1,2 env=CLAUDE_CODE_EXECUTABLE,HOME,PATH,TERM");
    // The leader of a session and a process group of its own.
    assert_eq!([process.sid, process.pgid], [process.pid; 2]);
    let sql = "SELECT exit_status FROM adapters WHERE ended_at IS NOT NULL";
    let ended: Vec<String> = query(&dir.host_db(), sql, |row| row.get(0));
    assert_eq!(ended, ["signal: 9 (SIGKILL)"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k13_echo_cwd_the_agent_gets_the_real_path() {
    let dir = TempDir::new("k13");
    let real = fs::canonicalize(dir.path()).unwrap();
    // `TMPDIR` is under `/var`, a link to `/private/var`.
    assert_ne!(real, dir.path());
    let (said, _) = run(&dir, dir.path(), "echo_env").await;
    let cwd = format!("cwd={} ", real.display());
    assert!(said.starts_with(&cwd), "{said}");
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k26_client_info_names_cupper_only() {
    let dir = TempDir::new("k26");
    let (said, _) = run(&dir, dir.path(), "echo_env").await;
    let (_, rest) = said.split_once(" clientInfo=").unwrap();
    let client: serde_json::Value =
        serde_json::from_str(rest.split_once(" env=").unwrap().0).unwrap();
    assert_eq!(client["name"], "cupper");
}
