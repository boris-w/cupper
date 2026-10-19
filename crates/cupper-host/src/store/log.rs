//! The event log's tables in `host.db` (design 5.4.2): `events`,
//! `checkpoints` and `turn_snapshots`. Appends write them on the writer
//! thread; loads and history pages read them on the read pool.
//!
//! What is read goes through the upcasters first (design 5.4.6): an event
//! and every op of a turn snapshot carry the `v` of their type as written,
//! and come out at the current version.

use std::sync::Arc;

use cupper_protocol::envelope::Event;
use cupper_protocol::event::{EventBody, Strictness};
use cupper_protocol::ids::{SessionId, TurnId};
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::{OpLimits, SessionView, VIEW_VERSION};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};

use super::streams::{Published, Retention, StreamError};
use super::upcast::Upcasters;

/// `turn_snapshots.v`, how `ops` is laid out: 1 is a JSON array of
/// `{type, v, data}`. Event versions go by type, so each op carries its own
/// type's version and is upcast on its own, like an event (T026).
pub const TURN_SNAPSHOT_LAYOUT: u32 = 1;

pub(super) fn insert_events(
    conn: &Connection,
    stream: &str,
    events: &[Arc<Published>],
) -> rusqlite::Result<()> {
    let mut insert = conn.prepare(
        "INSERT INTO events (stream, seq, type, v, ts, json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for published in events {
        let event = &published.event;
        let json = &*published.json;
        insert.execute((
            stream,
            event.seq,
            event.body.tag(),
            event.v,
            event.ts.0,
            json,
        ))?;
    }
    Ok(())
}

/// Calls `each` with the events of `stream` from `after + 1` to `up_to`, in
/// order, at their current versions: a stream's load, a subscriber's replay.
pub fn read_events(
    conn: &Connection,
    stream: &StreamId,
    after: u64,
    up_to: u64,
    mut each: impl FnMut(Event<EventBody>),
) -> Result<(), StreamError> {
    let mut select = conn.prepare(
        "SELECT type, v, json FROM events WHERE stream = ?1 AND seq > ?2 AND seq <= ?3
         ORDER BY seq",
    )?;
    let mut rows = select.query((stream.to_string(), after, up_to))?;
    while let Some(row) = rows.next()? {
        let (kind, v, json): (String, u32, String) = (row.get(0)?, row.get(1)?, row.get(2)?);
        let mut event: Value = serde_json::from_str(&json)?;
        upcast(&mut event, &kind, v)?;
        each(Event::decode(&event, Strictness::Strict)?.value);
    }
    Ok(())
}

/// The oldest seq of `stream` the log keeps.
pub(super) fn first_seq(conn: &Connection, stream: &str) -> rusqlite::Result<Option<u64>> {
    conn.query_row(
        "SELECT min(seq) FROM events WHERE stream = ?1",
        [stream],
        |row| row.get(0),
    )
}

/// Drops what retention lets go once a checkpoint stands at `at_seq` (design
/// 5.4.5): it keeps every event after the checkpoint, the `retention.events`
/// before it, and every event younger than `retention.age`. The cut stops
/// before the oldest young event, so what is kept stays contiguous. Returns
/// the cut: no event up to it is left.
pub(super) fn prune(
    conn: &Connection,
    stream: &str,
    at_seq: u64,
    retention: &Retention,
    now_ms: u64,
) -> rusqlite::Result<Option<u64>> {
    let Some(cut) = at_seq.checked_sub(retention.events) else {
        return Ok(None);
    };
    let age_ms = u64::try_from(retention.age.as_millis()).unwrap_or(u64::MAX);
    let young: Option<u64> = conn.query_row(
        "SELECT min(seq) FROM events WHERE stream = ?1 AND seq <= ?2 AND ts >= ?3",
        (stream, cut, now_ms.saturating_sub(age_ms)),
        |row| row.get(0),
    )?;
    let cut = young.map_or(cut, |young| cut.min(young - 1));
    conn.execute(
        "DELETE FROM events WHERE stream = ?1 AND seq <= ?2",
        (stream, cut),
    )?;
    Ok(Some(cut))
}

/// The checkpoint of `stream`: `(at_seq, view_v, view)`.
pub(super) fn checkpoint(
    conn: &Connection,
    stream: &str,
) -> rusqlite::Result<Option<(u64, u32, String)>> {
    conn.query_row(
        "SELECT at_seq, view_v, view FROM checkpoints WHERE stream = ?1",
        [stream],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
}

pub(super) fn save_checkpoint(
    conn: &Connection,
    stream: &str,
    at_seq: u64,
    view: &str,
    now_ms: u64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO checkpoints (stream, at_seq, view_v, view, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (stream) DO UPDATE SET at_seq = excluded.at_seq,
           view_v = excluded.view_v, view = excluded.view, created_at = excluded.created_at",
        (stream, at_seq, VIEW_VERSION, view, now_ms),
    )
    .map(drop)
}

/// A turn as it completed: what `history/page` serves (design 5.2.8).
#[derive(Debug)]
pub(super) struct TurnSnapshot {
    session: SessionId,
    turn_id: TurnId,
    turn_no: u32,
    ops: String,
}

impl TurnSnapshot {
    /// `turn` as `view` has it, cut by [`OpLimits::V1`].
    pub fn of(
        session: &SessionId,
        view: &SessionView,
        turn: &TurnId,
    ) -> Option<serde_json::Result<Self>> {
        let started = &view
            .turns
            .iter()
            .find(|t| t.started.turn_id == *turn)?
            .started;
        let ops = view.turn_ops(turn, &OpLimits::V1)?;
        let upcasters = Upcasters::host();
        let encode = || {
            let ops = ops.iter().map(|op| {
                let mut op = serde_json::to_value(op)?;
                let v = upcasters.current(&kind_of(&op));
                op["v"] = json!(v);
                Ok(op)
            });
            serde_json::to_string(&ops.collect::<serde_json::Result<Vec<_>>>()?)
        };
        Some(encode().map(|ops| Self {
            session: session.clone(),
            turn_id: turn.clone(),
            turn_no: started.turn_no,
            ops,
        }))
    }

    /// Fails if the turn has a snapshot already: it never changes.
    pub fn insert(&self, conn: &Connection) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO turn_snapshots (session_id, turn_id, turn_no, v, ops, bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                self.session.as_str(),
                self.turn_id.as_str(),
                self.turn_no,
                TURN_SNAPSHOT_LAYOUT,
                &self.ops,
                self.ops.len(),
            ),
        )
        .map(drop)
    }
}

/// The ops of turn `turn_no`, at their current versions.
pub(super) fn turn_ops(
    conn: &Connection,
    session: &SessionId,
    turn_no: u32,
) -> Result<Option<Vec<EventBody>>, StreamError> {
    let row = conn
        .query_row(
            "SELECT v, ops FROM turn_snapshots WHERE session_id = ?1 AND turn_no = ?2",
            (session.as_str(), turn_no),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    row.map(|(layout, ops): (u32, String)| decode_ops(layout, &ops))
        .transpose()
}

/// The newest turn before `turn_no` that has a snapshot.
pub(super) fn turn_before(
    conn: &Connection,
    session: &SessionId,
    turn_no: u32,
) -> rusqlite::Result<Option<u32>> {
    conn.query_row(
        "SELECT max(turn_no) FROM turn_snapshots WHERE session_id = ?1 AND turn_no < ?2",
        (session.as_str(), turn_no),
        |row| row.get(0),
    )
}

/// The ops of the latest `turns` turn snapshots, oldest turn first.
pub(super) fn latest_turns(
    conn: &Connection,
    session: &SessionId,
    turns: usize,
) -> Result<Vec<EventBody>, StreamError> {
    let mut select = conn.prepare(
        "SELECT v, ops FROM (SELECT turn_no, v, ops FROM turn_snapshots WHERE session_id = ?1
         ORDER BY turn_no DESC LIMIT ?2) ORDER BY turn_no",
    )?;
    let mut rows = select.query((session.as_str(), turns))?;
    let mut ops = Vec::new();
    while let Some(row) = rows.next()? {
        ops.extend(decode_ops(row.get(0)?, &row.get::<_, String>(1)?)?);
    }
    Ok(ops)
}

fn decode_ops(layout: u32, ops: &str) -> Result<Vec<EventBody>, StreamError> {
    if layout != TURN_SNAPSHOT_LAYOUT {
        return Err(StreamError::Corrupt(format!(
            "turn snapshot layout {layout}"
        )));
    }
    let ops: Vec<Value> = serde_json::from_str(ops)?;
    ops.into_iter()
        .map(|mut op| {
            let v = op["v"].as_u64().and_then(|v| u32::try_from(v).ok());
            let v =
                v.ok_or_else(|| StreamError::Corrupt("a turn snapshot op without `v`".into()))?;
            let kind = kind_of(&op);
            upcast(&mut op, &kind, v)?;
            Ok(EventBody::decode(&op, Strictness::Strict)?.value)
        })
        .collect()
}

/// Brings the `data` of `event`, of type `kind` at version `v`, to the
/// current version.
fn upcast(event: &mut Value, kind: &str, v: u32) -> Result<(), StreamError> {
    let upcasters = Upcasters::host();
    let data = upcasters.upcast(kind, v, event["data"].take())?;
    event["data"] = data;
    event["v"] = json!(upcasters.current(kind));
    Ok(())
}

fn kind_of(body: &Value) -> String {
    body["type"].as_str().unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use cupper_protocol::view::View;

    use super::*;
    use crate::store::testutil::{end, host_db, item, project, published, turn};

    fn seqs(conn: &Connection, stream: &StreamId, after: u64) -> Result<Vec<u64>, StreamError> {
        let mut seqs = Vec::new();
        read_events(conn, stream, after, 9, |event| seqs.push(event.seq))?;
        Ok(seqs)
    }

    #[test]
    fn store_log_events_read_back_upcast_and_prune_contiguously() {
        let conn = host_db();
        // Seq 5 is young; the clock went back before 6.
        let ts = [0, 0, 0, 0, 100, 0, 0, 0, 0, 0];
        let events: Vec<_> = (1..=10)
            .zip(ts)
            .map(|(seq, ts)| published(seq, ts, project(seq)))
            .collect();
        insert_events(&conn, "index", &events).unwrap();
        assert_eq!(seqs(&conn, &StreamId::Index, 7).unwrap(), [8, 9]);

        let keep = |events| Retention {
            events,
            age: Duration::from_millis(50),
        };
        assert_eq!(prune(&conn, "index", 10, &keep(11), 120).unwrap(), None);
        assert_eq!(prune(&conn, "index", 10, &keep(3), 120).unwrap(), Some(4));
        assert_eq!(first_seq(&conn, "index").unwrap(), Some(5));

        // A version this host does not know fails the read.
        conn.execute("UPDATE events SET v = 2 WHERE seq = 6", [])
            .unwrap();
        assert!(matches!(
            seqs(&conn, &StreamId::Index, 0),
            Err(StreamError::Upcast(_))
        ));
    }

    #[test]
    fn store_log_turn_snapshots_are_written_once_and_read_at_current_versions() {
        let conn = host_db();
        let session = "s_1".parse().unwrap();
        let mut view = SessionView::default();
        for body in [turn(1), item("i_1", 1), end(1), turn(2), end(2)] {
            view.apply(&body).unwrap();
        }
        for n in 1..=2 {
            let id = format!("t_{n}").parse().unwrap();
            let snapshot = TurnSnapshot::of(&session, &view, &id).unwrap().unwrap();
            snapshot.insert(&conn).unwrap();
            assert!(
                snapshot.insert(&conn).is_err(),
                "a turn snapshot never changes"
            );
            assert_eq!(
                turn_ops(&conn, &session, n).unwrap(),
                view.turn_ops(&id, &OpLimits::V1)
            );
        }
        assert_eq!(turn_before(&conn, &session, 2).unwrap(), Some(1));
        assert_eq!(turn_before(&conn, &session, 1).unwrap(), None);
        assert_eq!(latest_turns(&conn, &session, 1).unwrap(), [turn(2), end(2)]);
        assert_eq!(turn_ops(&conn, &session, 3).unwrap(), None);

        conn.execute("UPDATE turn_snapshots SET v = 2 WHERE turn_no = 1", [])
            .unwrap();
        assert!(matches!(
            turn_ops(&conn, &session, 1),
            Err(StreamError::Corrupt(_))
        ));

        save_checkpoint(&conn, "index", 3, "{}", 1).unwrap();
        save_checkpoint(&conn, "index", 7, "[]", 2).unwrap();
        assert_eq!(
            checkpoint(&conn, "index").unwrap(),
            Some((7, VIEW_VERSION, "[]".into()))
        );
        assert_eq!(checkpoint(&conn, "session/s_1").unwrap(), None);
    }
}
