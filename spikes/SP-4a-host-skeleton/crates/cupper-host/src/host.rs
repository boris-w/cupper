//! Host state shared by connections and session actors, plus crash recovery.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cupper_protocol::rpc::{ErrorCode, RpcError};
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::StreamView;
use cupper_protocol::{
    EventBody, InteractionResolved, InteractionStatus, SessionStatus, SessionSummary, TurnCompleted, TurnStatus,
};

use crate::commands::Commands;
use crate::driver::AgentSpec;
use crate::hub::Hub;
use crate::session::{SessionActor, SessionHandle, SessionMsg};
use crate::store::{SessionRecord, Store};
use crate::sys;
use crate::util::{new_id, now_ms};

#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub agents: HashMap<String, AgentSpec>,
    /// Events kept in the log per stream beyond the latest checkpoint.
    pub retain_events: u64,
    /// Per-connection live event backlog before a stream reset.
    pub max_queue_bytes: usize,
    /// Write-time coalescing window for text fragments.
    pub write_coalesce: Duration,
    /// Key for the `test_harness` principal (test-harness builds only).
    pub harness_key: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct Host {
    pub config: Config,
    pub host_id: String,
    pub store: Arc<Store>,
    pub hub: Hub,
    pub commands: Commands,
    sessions: Mutex<HashMap<String, SessionHandle>>,
    next_conn: AtomicU64,
}

impl Host {
    pub fn new(config: Config, store: Arc<Store>) -> Result<Arc<Self>, crate::store::StoreError> {
        let host_id = store.host_id(|| new_id("h"))?;
        let hub = Hub::new(store.clone(), config.retain_events);
        let commands = Commands::new(store.clone());
        Ok(Arc::new(Self { config, host_id, store, hub, commands, sessions: Mutex::default(), next_conn: AtomicU64::new(1) }))
    }

    pub fn next_conn_id(&self) -> u64 {
        self.next_conn.fetch_add(1, Ordering::Relaxed)
    }

    /// Update the session's row in the index stream.
    pub fn publish_session(&self, rec: &SessionRecord, status: SessionStatus, pending: u32) {
        let title = self
            .hub
            .with_view(&StreamId::Index, |v, _| match v {
                StreamView::Index(i) => i.session(&rec.session_id).and_then(|s| s.title.clone()),
                _ => None,
            })
            .ok()
            .flatten();
        let summary = SessionSummary {
            session_id: rec.session_id.clone(),
            agent: rec.agent.clone(),
            cwd: rec.cwd.clone(),
            title,
            status,
            pending_interactions: pending,
            updated_at: now_ms(),
        };
        if let Err(e) = self.hub.append(&StreamId::Index, vec![EventBody::SessionUpsert(summary)], None) {
            tracing::error!("index update failed: {e}");
        }
    }

    pub async fn create_session(self: &Arc<Self>, agent: &str, cwd: &str) -> Result<String, RpcError> {
        if !self.config.agents.contains_key(agent) {
            return Err(RpcError::app(ErrorCode::NotFound, format!("unknown agent {agent}")).with_reason("unknown_agent"));
        }
        if !std::path::Path::new(cwd).is_dir() {
            return Err(RpcError::new(cupper_protocol::rpc::INVALID_PARAMS, "cwd is not a directory"));
        }
        let rec = SessionRecord {
            session_id: new_id("s"),
            agent: agent.to_owned(),
            cwd: cwd.to_owned(),
            agent_session_id: None,
            running: false,
        };
        self.store.insert_session(&rec, now_ms()).map_err(|e| RpcError::app(ErrorCode::Internal, e.to_string()))?;
        self.publish_session(&rec, SessionStatus::Starting, 0);
        let handle = SessionActor::spawn(self.clone(), rec.clone(), SessionStatus::Starting);
        self.sessions.lock().expect("sessions mutex poisoned").insert(rec.session_id.clone(), handle.clone());
        handle.call(|reply| SessionMsg::Start { reply }).await?;
        Ok(rec.session_id)
    }

    /// The session's actor, started on first use (e.g. after a host restart).
    pub fn session(self: &Arc<Self>, session_id: &str) -> Result<SessionHandle, RpcError> {
        let mut map = self.sessions.lock().expect("sessions mutex poisoned");
        if let Some(h) = map.get(session_id) {
            return Ok(h.clone());
        }
        let rec = self
            .store
            .session(session_id)
            .map_err(|e| RpcError::app(ErrorCode::Internal, e.to_string()))?
            .ok_or_else(|| RpcError::app(ErrorCode::NotFound, "no such session"))?;
        let status = self
            .hub
            .with_view(&StreamId::Index, |v, _| match v {
                StreamView::Index(i) => i.session(session_id).map(|s| s.status.clone()),
                _ => None,
            })
            .ok()
            .flatten()
            .unwrap_or(SessionStatus::Idle);
        let handle = SessionActor::spawn(self.clone(), rec, status);
        map.insert(session_id.to_owned(), handle.clone());
        Ok(handle)
    }

    pub fn session_exists(&self, session_id: &str) -> bool {
        self.store.session(session_id).ok().flatten().is_some()
    }

    pub fn shutdown_sessions(&self) {
        for h in self.sessions.lock().expect("sessions mutex poisoned").values() {
            h.shutdown();
        }
    }

    /// NFR-9 recovery after a crash: reap leftover adapters, close running turns
    /// as interrupted and pending requests as expired. Sessions resume lazily on
    /// the next `session/send`.
    pub fn recover(&self) -> Result<RecoveryReport, crate::hub::HubError> {
        let mut report = RecoveryReport { reaped_processes: self.reap_adapters()?, ..Default::default() };
        for s in self.store.sessions()? {
            if !s.running {
                continue;
            }
            let stream = StreamId::Session(s.session_id.clone());
            let (turn, pending) = self.hub.with_view(&stream, |v, _| match v {
                StreamView::Session(v) => (
                    v.running_turn().map(|t| t.turn_id.clone()),
                    v.pending_interactions().map(|i| i.request_id.clone()).collect::<Vec<_>>(),
                ),
                _ => (None, Vec::new()),
            })?;
            let mut bodies: Vec<EventBody> = pending
                .into_iter()
                .map(|request_id| {
                    report.expired_interactions += 1;
                    EventBody::InteractionResolved(InteractionResolved { request_id, status: InteractionStatus::Expired, outcome: None, resolved_by: None })
                })
                .collect();
            if let Some(turn_id) = turn {
                report.interrupted_turns += 1;
                bodies.push(EventBody::TurnCompleted(TurnCompleted {
                    turn_id,
                    status: TurnStatus::Interrupted,
                    stop_reason: Some("host_restarted".into()),
                }));
            }
            self.hub.append(&stream, bodies, None)?;
            self.store.set_running(&s.session_id, false)?;
            self.publish_session(&s, SessionStatus::Interrupted, 0);
            self.hub.checkpoint_and_prune(&stream)?;
        }
        Ok(report)
    }
}

impl Host {
    /// Kill every recorded adapter process group and forget the records. Used
    /// at startup (leftovers of a crash) and on graceful shutdown.
    pub fn reap_adapters(&self) -> Result<usize, crate::store::StoreError> {
        let mut reaped = 0;
        for a in self.store.adapters()? {
            reaped += reap_group(a.pid, a.start_us);
            self.store.remove_adapter(a.pid)?;
        }
        Ok(reaped)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RecoveryReport {
    pub reaped_processes: usize,
    pub interrupted_turns: usize,
    pub expired_interactions: usize,
}

/// Kill what is left of an adapter's process group. A member counts as ours if
/// it started no earlier than the recorded leader; a live leader must match the
/// recorded start time exactly (otherwise the pid was reused).
fn reap_group(pgid: i32, leader_start_us: u64) -> usize {
    let members: Vec<(i32, u64)> = sys::group_members(pgid);
    let leader_reused = members.iter().any(|(pid, start)| *pid == pgid && *start != leader_start_us);
    let ours = !leader_reused && !members.is_empty() && members.iter().all(|(_, start)| *start >= leader_start_us);
    if !ours {
        return 0;
    }
    tracing::info!(pgid, count = members.len(), "reaping adapter process group");
    let _ = sys::kill_group(pgid, libc::SIGTERM);
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline && !sys::group_members(pgid).is_empty() {
        std::thread::sleep(Duration::from_millis(20));
    }
    if !sys::group_members(pgid).is_empty() {
        let _ = sys::kill_group(pgid, libc::SIGKILL);
    }
    members.len()
}
