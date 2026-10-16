//! Test harness: a host process on a temp data dir with the fake agent, and
//! clients that keep a checked mirror of each stream.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use cupper_cli::{Client, ClientError, Incoming, Mirror, command_id};
use cupper_protocol::frame::MAX_FRAME_BYTES;
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::SessionView;
use cupper_protocol::*;

pub const TIMEOUT: Duration = Duration::from_secs(30);

fn bin_dir() -> PathBuf {
    Path::new(env!("CARGO_BIN_EXE_cupper-host")).parent().expect("bin dir").to_path_buf()
}

/// The fake agent binary, built on demand when only this package is tested.
pub fn fake_agent() -> PathBuf {
    let path = bin_dir().join("cupper-fake-agent");
    if !path.exists() {
        let release = bin_dir().ends_with("release");
        let mut cmd = Command::new(env!("CARGO"));
        cmd.args(["build", "-p", "cupper-fake-agent"]);
        if release {
            cmd.arg("--release");
        }
        assert!(cmd.status().expect("cargo build").success(), "building the fake agent failed");
    }
    path
}

pub struct Harness {
    pub dir: tempfile::TempDir,
    key: String,
    extra_args: Vec<String>,
    pub host: Option<Child>,
}

impl Harness {
    pub fn new(extra_args: &[&str]) -> Self {
        let dir = tempfile::Builder::new().prefix("cupper-test").tempdir().expect("tempdir");
        let agents = serde_json::json!({ "agents": { "fake": { "command": fake_agent() } } });
        std::fs::write(dir.path().join("agents.json"), agents.to_string()).expect("write agents.json");
        let key = hex(&rand_bytes());
        let mut h = Self { dir, key, extra_args: extra_args.iter().map(|s| s.to_string()).collect(), host: None };
        h.start();
        h
    }

    pub fn data_dir(&self) -> &Path {
        self.dir.path()
    }

    pub fn socket(&self) -> PathBuf {
        cupper_protocol::transport::socket_path(self.data_dir(), cupper_host::sys::current_uid())
    }

    /// Spawn the host, hand it the key over stdin, and wait until it accepts.
    pub fn start(&mut self) -> Duration {
        let t0 = Instant::now();
        self.spawn();
        self.wait_ready(Duration::from_millis(5));
        t0.elapsed()
    }

    /// Spawn the host without waiting for it.
    pub fn spawn(&mut self) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cupper-host"))
            .arg("--data-dir")
            .arg(self.data_dir())
            .arg("--harness-key-stdin")
            .args(&self.extra_args)
            .env("CUPPER_LOG", std::env::var("CUPPER_LOG").unwrap_or_else(|_| "warn".into()))
            .stdin(Stdio::piped())
            .spawn()
            .expect("spawn host");
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(self.key.as_bytes()).expect("write key");
        drop(stdin);
        self.host = Some(child);
    }

    /// Poll until the socket accepts connections.
    pub fn wait_ready(&mut self, poll: Duration) {
        let deadline = Instant::now() + TIMEOUT;
        while std::os::unix::net::UnixStream::connect(self.socket()).is_err() {
            assert!(Instant::now() < deadline, "host did not start");
            if let Some(status) = self.host.as_mut().and_then(|c| c.try_wait().ok().flatten()) {
                panic!("host exited early: {status}");
            }
            std::thread::sleep(poll);
        }
    }

    pub fn host_pid(&self) -> i32 {
        self.host.as_ref().expect("host running").id() as i32
    }

    /// SIGKILL the host (no cleanup at all).
    pub fn kill9(&mut self) {
        let mut child = self.host.take().expect("host running");
        child.kill().expect("kill -9");
        child.wait().expect("reap host");
    }

    /// SIGTERM and wait (graceful shutdown).
    pub fn stop(&mut self) {
        if let Some(mut child) = self.host.take() {
            let _ = Command::new("kill").arg("-TERM").arg(child.id().to_string()).status();
            let _ = child.wait();
        }
    }

    pub async fn client(&self) -> TestClient {
        self.client_with(None).await
    }

    pub async fn client_with(&self, coalesce_ms: Option<u32>) -> TestClient {
        let c = Client::connect(&self.socket(), AuthParams::Harness { token: self.key.clone() }, coalesce_ms).await.expect("connect");
        TestClient { c, mirrors: HashMap::new() }
    }

    /// A same-uid client without the harness key: the read-only `cli` principal.
    pub async fn readonly_client(&self) -> TestClient {
        let c = Client::connect(&self.socket(), AuthParams::Peer, None).await.expect("connect");
        TestClient { c, mirrors: HashMap::new() }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.stop();
    }
}

fn rand_bytes() -> [u8; 32] {
    let mut b = [0u8; 32];
    let a = uuid::Uuid::new_v4();
    let c = uuid::Uuid::new_v4();
    b[..16].copy_from_slice(a.as_bytes());
    b[16..].copy_from_slice(c.as_bytes());
    b
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A client plus checked mirrors of the streams it subscribed to.
pub struct TestClient {
    pub c: Client,
    pub mirrors: HashMap<String, Mirror>,
}

impl TestClient {
    pub async fn create_session(&self) -> String {
        let cwd = std::env::temp_dir().canonicalize().expect("temp dir").display().to_string();
        self.c
            .request::<SessionCreate>(SessionCreateParams { meta: CommandMeta::new(command_id()), agent: "fake".into(), cwd })
            .await
            .expect("session/create")
            .session_id
    }

    pub async fn send(&self, session: &str, text: &str) -> String {
        self.send_with_id(session, text, &command_id()).await.expect("session/send").turn_id
    }

    pub async fn send_with_id(&self, session: &str, text: &str, id: &str) -> Result<SessionSendResult, ClientError> {
        self.c
            .request::<SessionSend>(SessionSendParams {
                meta: CommandMeta::new(id),
                session_id: session.into(),
                text: text.into(),
                expected_turn_id: None,
            })
            .await
    }

    pub async fn respond(&self, session: &str, request_id: &str, digest: &str, outcome: InteractionOutcome) -> Result<InteractionRespondResult, ClientError> {
        self.c
            .request::<InteractionRespond>(InteractionRespondParams {
                meta: CommandMeta::new(command_id()),
                session_id: session.into(),
                request_id: request_id.into(),
                request_digest: digest.into(),
                outcome,
            })
            .await
    }

    pub async fn subscribe(&mut self, stream: &str, after_seq: Option<u64>) -> SubscribeResult {
        let r = self.c.request::<Subscribe>(SubscribeParams { stream: stream.into(), after_seq }).await.expect("subscribe");
        let m = self.mirrors.entry(stream.into()).or_insert_with(|| Mirror::new(stream.parse().expect("stream id")));
        m.on_subscribed(&r);
        r
    }

    pub fn mirror(&self, stream: &str) -> &Mirror {
        &self.mirrors[stream]
    }

    pub fn session_view(&self, session: &str) -> &SessionView {
        self.mirror(&StreamId::Session(session.into()).to_string()).session().expect("session view")
    }

    /// Process incoming data until `done` holds. Resets are answered by
    /// resubscribing from the mirror's cursor. Any invariant violation panics.
    pub async fn pump_until(&mut self, mut done: impl FnMut(&HashMap<String, Mirror>) -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        while !done(&self.mirrors) {
            let left = deadline.saturating_duration_since(Instant::now());
            let inc = self.c.next_timeout(left).await.unwrap_or_else(|e| panic!("waiting for data: {e}"));
            self.apply(inc).await;
        }
        assert!(self.c.max_frame_seen.load(Ordering::Relaxed) <= MAX_FRAME_BYTES);
    }

    /// Process whatever arrives within `quiet` of the previous message.
    pub async fn drain(&mut self, quiet: Duration) {
        while let Ok(inc) = self.c.next_timeout(quiet).await {
            self.apply(inc).await;
        }
    }

    async fn apply(&mut self, inc: Incoming) {
        match inc {
            Incoming::Events(p) => {
                let m = self.mirrors.get_mut(&p.stream).expect("events for a subscribed stream");
                m.on_events(&p).unwrap_or_else(|e| panic!("stream {}: {e}", p.stream));
            }
            Incoming::Snapshot(p) => {
                let m = self.mirrors.get_mut(&p.stream).expect("snapshot for a subscribed stream");
                m.on_snapshot(&p).unwrap_or_else(|e| panic!("stream {}: {e}", p.stream));
            }
            Incoming::Reset(p) => {
                let m = self.mirrors.get_mut(&p.stream).expect("reset for a subscribed stream");
                m.on_reset();
                let after = m.last_seq();
                self.subscribe(&p.stream, after).await;
            }
            Incoming::Other { method, params } => panic!("unexpected message {method}: {params}"),
            Incoming::BlobChunk(..) => {}
        }
    }

    /// Wait until the given turn is no longer running.
    pub async fn wait_turn(&mut self, session: &str, turn_id: &str) -> TurnStatus {
        let stream = StreamId::Session(session.into()).to_string();
        let t = turn_id.to_owned();
        self.pump_until(|m| {
            m.get(&stream).and_then(|m| m.session()).and_then(|s| s.turn(&t)).is_some_and(|t| t.status != TurnStatus::Running)
        })
        .await;
        self.session_view(session).turn(turn_id).expect("turn").status.clone()
    }

    /// Wait for a pending interaction in the session and return (request_id, digest).
    pub async fn wait_pending(&mut self, session: &str) -> (String, String) {
        let stream = StreamId::Session(session.into()).to_string();
        self.pump_until(|m| m.get(&stream).and_then(|m| m.session()).is_some_and(|s| s.pending_interactions().next().is_some())).await;
        let p = self.session_view(session).pending_interactions().next().expect("pending").clone();
        (p.request_id, p.digest)
    }

    /// Fetch a whole blob via `blob/get` and its `0x02` chunk frames.
    pub async fn fetch_blob(&mut self, hash: &str) -> Vec<u8> {
        let r = self.c.request::<BlobGet>(BlobGetParams { hash: hash.into(), offset: 0, length: None }).await.expect("blob/get");
        let mut data = Vec::new();
        let mut got = 0;
        while got < r.chunks {
            match self.c.next_timeout(TIMEOUT).await.expect("blob chunk") {
                Incoming::BlobChunk(h, bytes) => {
                    assert_eq!(h.offset, data.len() as u64);
                    data.extend_from_slice(&bytes);
                    got += 1;
                    assert_eq!(h.last, got == r.chunks);
                }
                other => self.apply(other).await,
            }
        }
        assert_eq!(data.len() as u64, r.length);
        data
    }
}

/// Session stream name.
pub fn ss(session: &str) -> String {
    StreamId::Session(session.into()).to_string()
}

/// Agent message text of one turn.
pub fn turn_text(v: &SessionView, turn_id: &str) -> String {
    v.items.iter().filter(|i| i.turn_id == turn_id && i.kind == ItemKind::AgentMessage).map(|i| i.text.as_str()).collect()
}

/// Resident set size of a process in KiB (via ps).
pub fn rss_kib(pid: i32) -> u64 {
    let out = Command::new("ps").args(["-o", "rss=", "-p", &pid.to_string()]).output().expect("ps");
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0)
}
