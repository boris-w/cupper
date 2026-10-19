//! A session's rows in `sessions`, `turns` and `queue_entries` (design
//! 5.4.2). Its actor is their one writer (design 5.2.3); they commit in the
//! transaction of the events they follow.

use std::collections::HashMap;
use std::fmt::Display;

use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{CommandId, QueueId, SessionId};
use cupper_protocol::session::TurnStatus;
use rusqlite::Connection;
use rusqlite::types::Value;
use serde::Serialize;

/// Statements with their parameters, to run in a batch's transaction.
#[derive(Debug, Default)]
pub(super) struct Rows(Vec<(String, Vec<Value>)>);

fn text(value: impl Display) -> Value {
    Value::Text(value.to_string())
}

fn int(value: impl Into<u64>) -> Value {
    Value::Integer(i64::try_from(value.into()).expect("seqs and times are below 2^53"))
}

fn json(value: &impl Serialize) -> Value {
    Value::Text(serde_json::to_string(value).expect("protocol values serialize"))
}

impl Rows {
    fn push(&mut self, sql: &str, params: Vec<Value>) {
        self.0.push((sql.to_owned(), params));
    }

    /// `UPDATE sessions SET <set>, updated_at = ?2 WHERE session_id = ?1`,
    /// `set` taking its values from `?3` on.
    fn session(&mut self, session: &SessionId, at: u64, set: &str, values: Vec<Value>) {
        let sql = format!("UPDATE sessions SET {set}, updated_at = ?2 WHERE session_id = ?1");
        let params = [text(session), int(at)].into_iter().chain(values);
        self.0.push((sql, params.collect()));
    }

    /// A new session's row, with its first message until the first turn
    /// starts (design 5.2.6). `values`: the session, the project, the agent,
    /// the workspace, the configuration, the first message, the title, and
    /// who set the tier: its creator.
    pub(super) fn created(values: [String; 8], at: u64) -> Self {
        let source = if values[6].is_empty() {
            "none"
        } else {
            "prompt"
        };
        let mut rows = Self::default();
        let params = values.into_iter().map(Value::Text);
        rows.push(
            "INSERT INTO sessions (session_id, project_id, agent, workspace_path, config_json,
                 initial_prompt_json, title, mode_set_by, title_source, workspace_kind, state,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'local', 'idle', ?10, ?10)",
            params.chain([text(source), int(at)]).collect(),
        );
        rows
    }

    /// The changes `bodies`, from seq `first_seq` on, make; `commands` names
    /// the command that queued each message.
    pub(super) fn of(
        session: &SessionId,
        first_seq: u64,
        at: u64,
        bodies: &[EventBody],
        commands: &HashMap<QueueId, CommandId>,
    ) -> Self {
        let mut rows = Self::default();
        let s = || text(session);
        for (seq, body) in (first_seq..).zip(bodies) {
            match body {
                EventBody::TurnStarted(t) => {
                    rows.push(
                        "INSERT INTO turns (session_id, turn_id, turn_no, origin, status,
                             first_seq, started_at)
                         VALUES (?1, ?2, ?3, ?4, 'running', ?5, ?6)",
                        vec![
                            s(),
                            text(&t.turn_id),
                            int(t.turn_no),
                            text(&t.origin),
                            int(seq),
                            int(t.started_at.0),
                        ],
                    );
                    // The first turn takes the first message (design 5.2.6).
                    let set = "state = 'running', initial_prompt_json = NULL";
                    rows.session(session, at, set, Vec::new());
                }
                EventBody::TurnCompleted(t) => {
                    let stop = t.stop_reason.as_ref().map_or(Value::Null, text);
                    let usage = t.usage.as_ref().map_or(Value::Null, json);
                    rows.push(
                        "UPDATE turns SET status = ?3, last_seq = ?4, ended_at = ?5,
                             stop_reason = ?6, usage_json = ?7
                         WHERE session_id = ?1 AND turn_id = ?2",
                        vec![
                            s(),
                            text(&t.turn_id),
                            text(&t.status),
                            int(seq),
                            int(t.ended_at.0),
                            stop,
                            usage,
                        ],
                    );
                    let state = match t.status {
                        TurnStatus::Failed => "error",
                        TurnStatus::Interrupted => "interrupted",
                        _ => "idle",
                    };
                    let set = "state = ?3, last_turn_end_seq = ?4";
                    rows.session(session, at, set, vec![text(state), int(seq)]);
                }
                EventBody::SnapshotRecorded(r) => rows.push(
                    "UPDATE turns SET snapshot_status = ?3 WHERE session_id = ?1 AND turn_id = ?2",
                    vec![s(), text(&r.turn_id), text(&r.status)],
                ),
                EventBody::QueueUpdated(q) => {
                    rows.push("DELETE FROM queue_entries WHERE session_id = ?1", vec![s()]);
                    for (position, e) in (0_u64..).zip(&q.entries) {
                        let command = commands.get(&e.queue_id).map(ToString::to_string);
                        let reply_to = e.reply_to.as_ref().map_or(Value::Null, text);
                        rows.push(
                            "INSERT INTO queue_entries (queue_id, session_id, position,
                                 content_json, principal, command_id, reply_to, created_at)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                            vec![
                                text(&e.queue_id),
                                s(),
                                int(position),
                                json(&e.content),
                                text(&e.created_by),
                                text(command.unwrap_or_default()),
                                reply_to,
                                int(e.created_at.0),
                            ],
                        );
                    }
                    let paused = q.paused.as_ref().map_or(Value::Null, |p| text(&p.reason));
                    let set = "queue_version = ?3, queue_paused = ?4";
                    rows.session(session, at, set, vec![int(q.version), paused]);
                }
                EventBody::SessionConfigured(c) => {
                    rows.session(session, at, "config_json = ?3", vec![json(&c.config)])
                }
                _ => {}
            }
        }
        rows
    }

    /// The agent's session to resume, from an attach.
    pub(super) fn agent_session(&mut self, session: &SessionId, at: u64, id: String) {
        self.session(session, at, "agent_session_id = ?3", vec![Value::Text(id)]);
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn write(&self, conn: &Connection) -> rusqlite::Result<()> {
        for (sql, params) in &self.0 {
            conn.execute(sql, rusqlite::params_from_iter(params))?;
        }
        Ok(())
    }
}
