//! A session's rows in `sessions`, `turns`, `queue_entries` and
//! `interactions` (design 5.4.2). Its actor is their one writer (design 5.2.3); they commit in the
//! transaction of the events they follow.

use std::collections::HashMap;
use std::fmt::Display;

use cupper_protocol::domain::Content;
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{AdapterInstanceId, CommandId, PrincipalId, QueueId, SessionId};
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

    /// Who set the session's tier, and, when `confirmed`, that full access
    /// was confirmed now (design 5.2.6).
    pub(super) fn set_by(
        &mut self,
        session: &SessionId,
        at: u64,
        by: &PrincipalId,
        confirmed: bool,
    ) {
        let set = "mode_set_by = ?3, \
            full_access_confirmed_at = CASE WHEN ?4 THEN ?2 ELSE full_access_confirmed_at END";
        self.session(
            session,
            at,
            set,
            vec![text(by), Value::Integer(confirmed.into())],
        );
    }

    /// The changes `bodies`, from seq `first_seq` on, make; `commands` names
    /// the command that queued each message, `instance` the adapter instance
    /// a request belongs to (SEC-10).
    pub(super) fn of(
        session: &SessionId,
        first_seq: u64,
        at: u64,
        bodies: &[EventBody],
        commands: &HashMap<QueueId, CommandId>,
        instance: Option<&AdapterInstanceId>,
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
                // `request_json` keeps the whole request as written, to answer
                // it again after a restart (T089).
                EventBody::InteractionRequested(r) => {
                    let kind = serde_json::to_value(&r.request).expect("protocol values serialize");
                    let kind = text(kind["type"].as_str().unwrap_or_default());
                    let (turn, item) = (r.turn_id.as_ref(), r.item_id.as_ref());
                    let instance = instance.map_or_else(|| text(""), text);
                    rows.push(
                        "INSERT INTO interactions (request_id, session_id, turn_id, item_id, kind,
                             digest, status, adapter_instance, request_json, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?8, ?9)",
                        vec![
                            text(&r.request_id),
                            s(),
                            turn.map_or(Value::Null, text),
                            item.map_or(Value::Null, text),
                            kind,
                            text(&r.digest),
                            instance,
                            json(r),
                            int(r.created_at.0),
                        ],
                    );
                }
                EventBody::InteractionResolved(r) => {
                    let outcome = r.outcome.as_ref().map_or(Value::Null, json);
                    let by = r.resolved_by.as_ref().map_or(Value::Null, json);
                    let reason = r.reason.as_ref().map_or(Value::Null, text);
                    rows.push(
                        "UPDATE interactions SET status = ?2, outcome_json = ?3, resolved_by = ?4,
                             reason = ?5, resolved_at = ?6
                         WHERE request_id = ?1",
                        vec![
                            text(&r.request_id),
                            text(&r.status),
                            outcome,
                            by,
                            reason,
                            int(at),
                        ],
                    );
                }
                // The session keeps the blob of a tool's output (T177).
                EventBody::ItemCompleted(c) => {
                    if let Some(Content::Blob(b)) = &c.patch.output {
                        rows.push(
                            "INSERT OR IGNORE INTO blob_refs (hash, owner) VALUES (?1, ?2)",
                            vec![text(&b.blob.hash), s()],
                        );
                    }
                }
                EventBody::SessionConfigured(c) => {
                    rows.session(session, at, "config_json = ?3", vec![json(&c.config)])
                }
                _ => {}
            }
        }
        rows
    }

    /// "不加载项目配置继续": the session leaves its project's configuration
    /// out for good, across restarts too.
    pub(super) fn ignored(&mut self, session: &SessionId, at: u64) {
        self.session(session, at, "project_config_ignored = 1", Vec::new());
    }

    /// The first message left the machine's hold for a turn or the queue.
    pub(super) fn first_taken(&mut self, session: &SessionId, at: u64) {
        self.session(session, at, "initial_prompt_json = NULL", Vec::new());
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
