//! `diag/recording_export` (design 5.9.3; `test-harness` and `dev-auth`
//! builds only): a session's raw frames as a recording in format v1, for the
//! compatibility suite's B-layer recordings. The recording is not redacted
//! beyond the raw frames' structured redaction: the caller runs
//! `tools/sanitize` before it goes into `testdata/`.
//!
//! Every adapter instance that carried the session is a segment, in the order
//! the instances started, without the frames that name another session. A
//! segment's `t` counts from its first frame, never going back.

use std::path::PathBuf;
use std::sync::Arc;

use cupper_protocol::error::{ErrorCode, INVALID_PARAMS, RpcError};
use cupper_protocol::ids::SessionId;
use cupper_protocol::methods::admin::{
    DiagExportResult, DiagRecordingExport, DiagRecordingExportParams,
};
use cupper_protocol::recording::{Body, Frame, Header, INSTANCE_STARTED, Recording, Source, Usage};
use futures::FutureExt;
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::gateway::{Call, Port, Reply, Routes};
use crate::store::{BlobStore, OpenError, open_reader};

/// The media type of an exported recording.
const JSONL: &str = "application/jsonl";

/// The method's port, on the stores of `data_dir`.
#[derive(Debug)]
pub struct RecordingExport {
    pub data_dir: PathBuf,
    pub blobs: BlobStore,
}

impl RecordingExport {
    pub fn route(self: Arc<Self>, routes: &mut Routes) {
        routes.add::<DiagRecordingExport>(self);
    }
}

impl Port for RecordingExport {
    fn deliver(&self, call: Call) -> Reply {
        let (dir, blobs) = (self.data_dir.clone(), self.blobs.clone());
        let internal =
            |err: &dyn std::fmt::Display| RpcError::app(ErrorCode::Internal, err.to_string());
        async move {
            let params = DiagRecordingExportParams::deserialize(&call.params)
                .map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))?;
            let session = params.session_id;
            let read = tokio::task::spawn_blocking(move || {
                let (host, raw) = (
                    open_reader(&dir.join("host.db"))?,
                    open_reader(&dir.join("raw.db"))?,
                );
                Ok::<_, OpenError>(recording(&host, &raw, &session)?)
            });
            let recording = read.await.map_err(|err| internal(&err))?;
            let recording = recording.map_err(|err| internal(&err))?;
            let recording =
                recording.ok_or_else(|| RpcError::app(ErrorCode::NotFound, "no such session"))?;
            let jsonl = recording.to_jsonl().into_bytes();
            let blob = blobs.put(jsonl, JSONL.to_owned(), None).await;
            let blob = blob.map_err(|err| internal(&err))?;
            Ok(json!(DiagExportResult { blob }))
        }
        .boxed()
    }
}

/// The recording of `session`; `None` when there is no such session.
pub fn recording(
    host: &Connection,
    raw: &Connection,
    session: &SessionId,
) -> rusqlite::Result<Option<Recording>> {
    let sql = "SELECT agent_session_id FROM sessions WHERE session_id = ?1";
    let found = host.query_row(sql, [session.as_str()], |row| {
        row.get::<_, Option<String>>(0)
    });
    let Some(agent_session) = found.optional()? else {
        return Ok(None);
    };
    let mut select = raw.prepare(
        "SELECT instance_id, seq, ts, dir, frame FROM raw_frames
         WHERE instance_id IN (SELECT instance_id FROM raw_frames WHERE agent_session_id = ?1)
           AND (agent_session_id IS NULL OR agent_session_id = ?1)
         ORDER BY instance_id, seq",
    )?;
    let rows = select.query_map([&agent_session], |row| {
        let raw: RawRow = (
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
        );
        Ok(raw)
    })?;
    let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut instances: Vec<Vec<RawRow>> = rows
        .chunk_by(|a, b| a.0 == b.0)
        .map(<[RawRow]>::to_vec)
        .collect();
    instances.sort_by_key(|rows| rows.iter().map(|row| row.2).min());
    let first = instances.first().map(|rows| rows[0].0.clone());
    let header = header(host, first.as_deref(), session)?;
    let frames = (0..).zip(&instances).flat_map(|(n, rows)| segment(n, rows));
    let frames = frames.collect();
    Ok(Some(Recording { header, frames }))
}

/// `instance_id, seq, ts, dir, frame` of `raw_frames`.
type RawRow = (String, u64, u64, String, String);

/// Instance `n`'s frames, beginning with its `instance_started` note; one is
/// made at `t` 0 if retention took the instance's own.
fn segment(n: u64, rows: &[RawRow]) -> Vec<Frame> {
    let start = rows.first().map_or(0, |row| row.2);
    let mut frames: Vec<Frame> = Vec::new();
    for (_, seq, ts, dir, frame) in rows {
        let Some(body) = body(dir, frame) else {
            continue;
        };
        let last = frames.last().map_or(0, |frame| frame.t);
        let (seq, t, instance) = (*seq, ts.saturating_sub(start).max(last), n);
        frames.push(Frame {
            seq,
            t,
            instance,
            body,
        });
    }
    if frames.first().and_then(|frame| frame.body.event()) != Some(INSTANCE_STARTED) {
        let note = json!({"event": INSTANCE_STARTED})
            .as_object()
            .cloned()
            .unwrap_or_default();
        let (seq, t, instance, body) = (0, 0, n, Body::Note(note));
        frames.insert(
            0,
            Frame {
                seq,
                t,
                instance,
                body,
            },
        );
    }
    frames
}

/// A raw frame's body; JSON that does not parse stays as data.
fn body(dir: &str, frame: &str) -> Option<Body> {
    let json = || serde_json::from_str::<Value>(frame).ok();
    Some(match dir {
        "send" => json().map_or_else(|| Body::RecvNonjson(frame.to_owned()), Body::Send),
        "recv" => json().map_or_else(|| Body::RecvNonjson(frame.to_owned()), Body::Recv),
        "recv_nonjson" => Body::RecvNonjson(frame.to_owned()),
        "stderr" => Body::Stderr(frame.to_owned()),
        "note" => Body::Note(json()?.as_object()?.clone()),
        _ => return None,
    })
}

/// The header, from the `adapters` row of the first instance. The scenario
/// is the session until the compatibility suite names it (T064).
fn header(host: &Connection, first: Option<&str>, session: &SessionId) -> rusqlite::Result<Header> {
    let sql = "SELECT agent, adapter_version, cli_version FROM adapters WHERE instance_id = ?1";
    let row = first.map(|first| {
        host.query_row(sql, [first], |row| {
            let row: (String, String, Option<String>) = (row.get(0)?, row.get(1)?, row.get(2)?);
            Ok(row)
        })
        .optional()
    });
    let (agent, adapter_version, cli_version) = row.transpose()?.flatten().unwrap_or_default();
    let adapter = match agent.as_str() {
        "codex" => "codex-acp",
        _ => "claude-agent-acp",
    };
    let cli = if agent.is_empty() { "claude" } else { &agent };
    Ok(Header {
        source: Source::Host,
        adapter: adapter.to_owned(),
        adapter_version,
        cli: cli.to_owned(),
        cli_version: cli_version.unwrap_or_default(),
        scenario: session.to_string(),
        usage: vec![Usage::GoldB, Usage::Replay],
    })
}

#[cfg(test)]
mod tests {
    use cupper_protocol::recording::{Dir, INSTANCE_EXITED};

    use super::*;
    use crate::store::testutil::{TempDir, host_db, raw_db};

    /// `seq`, `t`, `dir` and a note's event.
    type Row<'a> = (u64, u64, Dir, Option<&'a str>);

    /// Two instances of session `s-1` beside one of `s-2`'s frames.
    #[test]
    fn it_recording_export_segments_instances_and_times_frames() {
        let host = host_db();
        let session = SessionId::from_uuid(1);
        host.execute_batch(
            "INSERT INTO projects (project_id, path, name, position, added_at)
               VALUES ('p_1', '/p', 'p', 0, 1);
             INSERT INTO adapters (instance_id, agent, pid, pgid, start_us, adapter_version,
                 node_version, cli_version, started_at)
               VALUES ('a_2', 'claude', 1, 1, 1, '0.91.0', '24', '2.1.296', 1000);",
        )
        .unwrap();
        let sql = "INSERT INTO sessions (session_id, project_id, agent, workspace_kind,
                       workspace_path, state, created_at, updated_at, agent_session_id)
                   VALUES (?1, 'p_1', 'claude', 'local', '/p', 'idle', 1, 1, 's-1')";
        host.execute(sql, [session.to_string()]).unwrap();
        let dir = TempDir::new("recording-export");
        let raw = raw_db(&dir);
        // Instance `a_9` lost its start note to retention; `a_7` is `s-2`'s.
        raw.execute_batch(
            r#"INSERT INTO raw_frames VALUES
               ('a_9', 2, 5100, 'recv', 's-1', '{"params":{"sessionId":"s-1"}}'),
               ('a_2', 1, 1000, 'note', NULL, '{"event":"instance_started"}'),
               ('a_2', 2, 1010, 'send', 's-1', '{"params":{"sessionId":"s-1"}}'),
               ('a_2', 3, 1005, 'stderr', NULL, 'oops'),
               ('a_2', 4, 1020, 'recv', 's-2', '{"params":{"sessionId":"s-2"}}'),
               ('a_2', 5, 1030, 'note', NULL, '{"event":"instance_exited","code":0}'),
               ('a_7', 1, 1, 'recv', 's-2', '{}');"#,
        )
        .unwrap();
        let exported = recording(&host, &raw, &session).unwrap().unwrap();
        let parsed = Recording::parse(&exported.to_jsonl()).unwrap();
        let header = &parsed.header;
        let named = [
            &header.adapter,
            &header.adapter_version,
            &header.cli_version,
        ];
        assert_eq!(named, ["claude-agent-acp", "0.91.0", "2.1.296"]);
        let segments: Vec<Vec<Row<'_>>> = parsed
            .segments()
            .map(|frames| {
                (frames.iter())
                    .map(|f| (f.seq, f.t, f.body.dir(), f.body.event()))
                    .collect()
            })
            .collect();
        let started = Some(INSTANCE_STARTED);
        assert_eq!(
            segments,
            [
                vec![
                    (1, 0, Dir::Note, started),
                    (2, 10, Dir::Send, None),
                    // The clock went back: the time stays.
                    (3, 10, Dir::Stderr, None),
                    (5, 30, Dir::Note, Some(INSTANCE_EXITED)),
                ],
                vec![(0, 0, Dir::Note, started), (2, 0, Dir::Recv, None)],
            ]
        );
        let unknown = SessionId::from_uuid(9);
        assert_eq!(recording(&host, &raw, &unknown).unwrap(), None);
    }
}
