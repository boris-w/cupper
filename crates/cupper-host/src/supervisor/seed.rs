//! `test/seed` (design 5.2.16, 5.9.9): synthetic sessions written the normal
//! way, through the stream writers and the writer thread, so that turn
//! snapshots and checkpoints come with them; then the index reconciles with
//! the tables and lists them. The sessions belong to the first project; add
//! one first.
//!
//! The `sp1` mix follows the 48 SP-1 recordings (computed once, T047 notes):
//! of the items an agent sends in a turn, 43% are messages of about 67
//! characters, 42% tool calls and 15% thoughts of about 44 characters.

use std::sync::Arc;

use cupper_protocol::error::{ErrorCode, INVALID_PARAMS, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{ItemId, ProjectId, SessionId, TurnId};
use cupper_protocol::methods::admin::{SeedProfile, TestSeedParams};
use cupper_protocol::sync::StreamId;
use futures::FutureExt;
use futures::future::BoxFuture;
use serde_json::{Value, json};

use crate::clock::{Clock, IdGen};
use crate::index::IndexHandle;
use crate::ledger::Outcome;
use crate::store::streams::{Batch, Streams};
use crate::store::{Durability, Readers};

/// Of every 20 items: 9 messages, 8 tool calls, 3 thoughts.
const MIX: [&str; 20] = [
    "agent_message",
    "tool_call",
    "agent_message",
    "tool_call",
    "thought",
    "agent_message",
    "tool_call",
    "agent_message",
    "tool_call",
    "agent_message",
    "thought",
    "tool_call",
    "agent_message",
    "tool_call",
    "agent_message",
    "tool_call",
    "thought",
    "agent_message",
    "tool_call",
    "agent_message",
];

#[derive(Debug, Clone)]
pub(super) struct Seeder {
    pub streams: Streams,
    pub readers: Readers,
    pub ids: Arc<IdGen>,
    pub clock: Arc<dyn Clock>,
    pub index: IndexHandle,
}

impl Seeder {
    pub fn seed(&self, params: Value) -> BoxFuture<'static, Outcome> {
        let seeder = self.clone();
        async move {
            let params: TestSeedParams = serde_json::from_value(params)
                .map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))?;
            seeder.run(&params.profile).await.map_err(|err| {
                RpcError::app(ErrorCode::Internal, format!("seeding failed: {err}"))
            })?;
            Ok(json!({}))
        }
        .boxed()
    }

    async fn run(&self, profile: &SeedProfile) -> Result<(), String> {
        let first = self.readers.read(|conn| {
            conn.query_row(
                "SELECT project_id, path FROM projects WHERE removed_at IS NULL
                 ORDER BY position LIMIT 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
        });
        let (project, path) = first.await.map_err(|_| "no project: add one first")?;
        let project: ProjectId = project.parse().map_err(|_| "a bad project id")?;
        for n in 0..profile.sessions {
            let large = (n == 0).then_some(profile.large_turn_items).flatten();
            self.session(&project, &path, profile, large).await?;
        }
        self.index.reconcile().await;
        Ok(())
    }

    /// One session of `profile`, and a last turn of `large` items.
    async fn session(
        &self,
        project: &ProjectId,
        path: &str,
        profile: &SeedProfile,
        large: Option<u32>,
    ) -> Result<(), String> {
        let id = SessionId::from_uuid(self.ids.new_id().as_u128());
        let handle = self.streams.open(&StreamId::Session(id.clone())).await;
        let mut writer = handle.and_then(|h| h.writer()).map_err(|e| e.to_string())?;
        let now = self.clock.now_wall_ms();
        let configured = body(json!({"type": "session.configured", "data": {
            "agent": "claude", "config": {}, "read_only": false, "project_config": "loaded",
            "workspace": {"kind": "local", "path": path}}}))?;
        let (row, project, path) = (id.to_string(), project.to_string(), path.to_owned());
        let batch = Batch {
            bodies: vec![configured],
            with: Some(Box::new(move |conn| {
                conn.execute(
                    "INSERT INTO sessions (session_id, project_id, agent, workspace_kind,
                         workspace_path, state, created_at, updated_at)
                     VALUES (?1, ?2, 'claude', 'local', ?3, 'idle', ?4, ?4)",
                    rusqlite::params![row, project, path, now],
                )
                .map(drop)
            })),
            ..Batch::default()
        };
        writer.append(batch).await.map_err(|e| e.to_string())?;
        let sizes = (0..profile.turns).map(|_| profile.items_per_turn);
        for (no, items) in (1..).zip(sizes.chain(large)) {
            let bodies = self.turn(no, items)?;
            let (first, last) = (writer.head() + 1, writer.head() + bodies.len() as u64);
            let (turn, session) = (bodies_turn(&bodies), id.to_string());
            let batch = Batch {
                bodies,
                durability: Durability::Normal,
                with: Some(Box::new(move |conn| {
                    conn.execute(
                        "INSERT INTO turns (session_id, turn_id, turn_no, origin, status,
                             first_seq, last_seq, started_at, ended_at)
                         VALUES (?1, ?2, ?3, 'user', 'completed', ?4, ?5, ?6, ?6)",
                        rusqlite::params![session, turn, no, first, last, now],
                    )?;
                    conn.execute(
                        "UPDATE sessions SET last_turn_end_seq = ?2, last_read_seq = ?2
                         WHERE session_id = ?1",
                        rusqlite::params![session, last],
                    )
                    .map(drop)
                })),
                ..Batch::default()
            };
            writer.append(batch).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Turn `no`: the user's message, `items` items of the mix, and its end.
    fn turn(&self, no: u32, items: u32) -> Result<Vec<EventBody>, String> {
        let now = self.clock.now_wall_ms();
        let new_id = || self.ids.new_id().as_u128();
        let turn = TurnId::from_uuid(new_id()).to_string();
        let started = |id: &str, kind: &str| json!({"item_id": id, "turn_id": turn, "started_at": now, "kind": kind});
        let mut events = vec![json!({"type": "turn.started", "data": {"turn_id": turn,
            "turn_no": no, "origin": "user", "started_at": now}})];
        let user = ItemId::from_uuid(new_id()).to_string();
        let mut message = started(&user, "user_message");
        message["content"] = json!([{"type": "text", "text": "Seeded request"}]);
        message["steered"] = json!(false);
        events.push(json!({"type": "item.started", "data": message}));
        let mut last_message = None;
        for (n, kind) in (0..items).zip(MIX.iter().cycle()) {
            let item = ItemId::from_uuid(new_id()).to_string();
            let mut data = started(&item, kind);
            let text = match *kind {
                "agent_message" => "Seeded answer text, about as long as an SP-1 message is. ",
                "thought" => "Seeded thought, about as long as SP-1's.",
                _ => {
                    let title = format!("Run step {n}");
                    data["tool_kind"] = json!("execute");
                    data["title"] = json!(title);
                    data["paths"] = json!([]);
                    data["output_state"] = json!("none");
                    ""
                }
            };
            events.push(json!({"type": "item.started", "data": data}));
            if !text.is_empty() {
                events.push(json!({"type": "item.delta", "data": {"item_id": item,
                    "field": "text", "text": text}}));
            }
            events.push(json!({"type": "item.completed", "data": {"item_id": item,
                "status": "completed", "ended_at": now, "duration_ms": 0}}));
            if *kind == "agent_message" {
                last_message = Some(item);
            }
        }
        events.push(json!({"type": "turn.completed", "data": {"turn_id": turn,
            "status": "completed", "ended_at": now, "duration_ms": 0,
            "final_item_ids": last_message.into_iter().collect::<Vec<_>>()}}));
        events.into_iter().map(body).collect()
    }
}

fn body(json: Value) -> Result<EventBody, String> {
    serde_json::from_value(json).map_err(|err| err.to_string())
}

/// The turn of a turn's events: the id of its first, `turn.started`.
fn bodies_turn(bodies: &[EventBody]) -> String {
    match bodies.first() {
        Some(EventBody::TurnStarted(started)) => started.turn_id.to_string(),
        _ => unreachable!("a turn starts with turn.started"),
    }
}
