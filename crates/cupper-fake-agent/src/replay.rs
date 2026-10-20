//! Replay mode (design 5.9.2): the fake agent plays an adapter from one
//! segment of a recording (5.9.3) instead of a script. Each message from the
//! host must match the next `send` frame; the frames after it go out in order,
//! their gaps cut to 1/[`SPEED`], until the next `send`. A request of the
//! adapter thus waits for the host's answer, and the answer must match too.
//! Lines are written verbatim, not through the SDK.
//!
//! A message matches when every field is equal, except that:
//! - JSON-RPC ids are remapped: the host's id of a request stands for the
//!   recorded one, and the recorded response goes back under the host's id;
//! - `cwd` and `path` fields are placeholders: a recorded path stands for the
//!   path the host sent first in its place, here and in the frames going out;
//! - `_meta` is ignored at every level.
//!
//! Everything else, the prompt text and the options of answers included,
//! must be equal. The segment ends at its `instance_exited` note, with the
//! recorded exit code.

use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap};
use std::io;
use std::time::Duration;

use cupper_protocol::recording::{Body, FormatError, Frame, INSTANCE_EXITED, Recording};
use serde_json::{Map, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::Instant;

/// Recorded gaps are cut to this fraction (design 5.9.2: 1/100).
pub const SPEED: u64 = 100;
/// Exit code when the host's messages do not match the recording.
pub const EXIT_MISMATCH: i32 = 71;
/// Fields that hold paths, compared as placeholders.
const PATH_FIELDS: [&str; 2] = ["cwd", "path"];

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("bad recording")]
    Format(#[from] FormatError),
    #[error("the recording has {count} segments, so no segment {segment}")]
    NoSegment { segment: u64, count: usize },
    #[error("frame {seq}: {diff}")]
    Mismatch { seq: u64, diff: Diff },
    #[error("frame {seq}: the host closed stdin before sending it")]
    Closed { seq: u64 },
    #[error("frame {seq}: the host sent a line that is not JSON: {line}")]
    NotJson { seq: u64, line: String },
    #[error("frame {seq}: it answers request {id}, which the host did not send")]
    Unanswered { seq: u64, id: Value },
    #[error("the host sent {0} after the recording's end")]
    AfterEnd(String),
    #[error("cannot talk to the host")]
    Io(#[from] io::Error),
}

/// Where a message differs from its frame.
#[derive(Debug, PartialEq, thiserror::Error)]
#[error("`{field}` is {actual}, the recording has {expected}")]
pub struct Diff {
    field: String,
    expected: String,
    actual: String,
}

/// Segment `segment` of a recording.
pub fn segment(text: &str, segment: u64) -> Result<Vec<Frame>, ReplayError> {
    let recording = Recording::parse(text)?;
    let count = recording.segments().count();
    let mut segments = recording.segments();
    let frames = usize::try_from(segment)
        .ok()
        .and_then(|n| segments.nth(n))
        .ok_or(ReplayError::NoSegment { segment, count })?;
    Ok(frames.to_vec())
}

/// Plays `frames` against the host; the exit code to leave with.
pub async fn play(
    frames: &[Frame],
    input: impl AsyncBufRead + Unpin,
    mut output: impl AsyncWrite + Unpin,
    mut errors: impl AsyncWrite + Unpin,
) -> Result<i32, ReplayError> {
    let mut lines = input.lines();
    let mut mapping = Mapping::default();
    // Gaps count from the last message of the host, as recorded and as now.
    let mut anchor = (now(), frames.first().map_or(0, |f| f.t));
    for frame in frames {
        let seq = frame.seq;
        if let Body::Send(expected) = &frame.body {
            let line = lines
                .next_line()
                .await?
                .ok_or(ReplayError::Closed { seq })?;
            let actual =
                serde_json::from_str(&line).map_err(|_| ReplayError::NotJson { seq, line })?;
            let diff = |diff| ReplayError::Mismatch { seq, diff };
            mapping.check(expected, &actual).map_err(diff)?;
            anchor = (now(), frame.t);
            continue;
        }
        let gap = frame.t.saturating_sub(anchor.1) * 1000 / SPEED;
        sleep_until(anchor.0 + Duration::from_micros(gap)).await;
        match &frame.body {
            Body::Recv(message) => {
                write(&mut output, &mapping.outgoing(seq, message)?.to_string()).await?
            }
            Body::RecvNonjson(line) => write(&mut output, line).await?,
            Body::Stderr(line) => write(&mut errors, line).await?,
            Body::Note(note) if frame.body.event() == Some(INSTANCE_EXITED) => {
                let code = note.get("code").and_then(Value::as_i64);
                return Ok(code.and_then(|c| i32::try_from(c).ok()).unwrap_or(1));
            }
            Body::Note(_) | Body::Send(_) => {}
        }
    }
    // The segment ends without an exit: stay until the host closes stdin.
    match lines.next_line().await? {
        Some(line) => Err(ReplayError::AfterEnd(line)),
        None => Ok(0),
    }
}

async fn write(to: &mut (impl AsyncWrite + Unpin), line: &str) -> io::Result<()> {
    to.write_all(format!("{line}\n").as_bytes()).await?;
    to.flush().await
}

/// What the host's messages stand for, learned as they match.
#[derive(Default)]
struct Mapping {
    /// The host's id of each recorded request, by the recorded id's JSON.
    ids: HashMap<String, Value>,
    /// Recorded path and the host's, longest recorded first.
    paths: Vec<(String, String)>,
}

impl Diff {
    fn new(field: &str, expected: Option<&Value>, actual: Option<&Value>) -> Self {
        let show = |v: Option<&Value>| v.map_or("nothing".to_owned(), Value::to_string);
        Diff {
            field: field.to_owned(),
            expected: show(expected),
            actual: show(actual),
        }
    }
}

impl Mapping {
    /// Whether the host's message matches the recorded one.
    fn check(&mut self, expected: &Value, actual: &Value) -> Result<(), Diff> {
        let (mut expected, mut actual) = (expected.clone(), actual.clone());
        let ids = match (expected.as_object_mut(), actual.as_object_mut()) {
            (Some(e), Some(a)) => (e.remove("id"), a.remove("id")),
            _ => (None, None),
        };
        let request = expected.get("method").is_some();
        self.compare("", &expected, &actual)?;
        match ids {
            (Some(recorded), Some(host)) if request => {
                self.ids.insert(recorded.to_string(), host);
                Ok(())
            }
            // Answers to the adapter's requests keep the adapter's ids.
            (recorded, host) if recorded == host => Ok(()),
            (recorded, host) => Err(Diff::new("id", recorded.as_ref(), host.as_ref())),
        }
    }

    fn compare(&mut self, field: &str, expected: &Value, actual: &Value) -> Result<(), Diff> {
        let at = |key: &dyn std::fmt::Display| match field {
            "" => key.to_string(),
            _ => format!("{field}.{key}"),
        };
        match (expected, actual) {
            (Value::Object(e), Value::Object(a)) => {
                let keys: BTreeSet<&String> = e.keys().chain(a.keys()).collect();
                for key in keys.into_iter().filter(|key| *key != "_meta") {
                    match (e.get(key), a.get(key)) {
                        (Some(Value::String(e)), Some(Value::String(a)))
                            if PATH_FIELDS.contains(&key.as_str()) =>
                        {
                            self.path(&at(key), e, a)?;
                        }
                        (Some(e), Some(a)) => self.compare(&at(key), e, a)?,
                        (e, a) => return Err(Diff::new(&at(key), e, a)),
                    }
                }
                Ok(())
            }
            (Value::Array(e), Value::Array(a)) if e.len() == a.len() => {
                for (n, (e, a)) in e.iter().zip(a).enumerate() {
                    self.compare(&format!("{field}[{n}]"), e, a)?;
                }
                Ok(())
            }
            _ if expected == actual => Ok(()),
            _ => Err(Diff::new(field, Some(expected), Some(actual))),
        }
    }

    fn path(&mut self, field: &str, recorded: &str, host: &str) -> Result<(), Diff> {
        match self.paths.iter().find(|(r, _)| r == recorded) {
            Some((_, known)) if known == host => Ok(()),
            Some((_, known)) => Err(Diff::new(
                field,
                Some(&known.as_str().into()),
                Some(&host.into()),
            )),
            None => {
                self.paths.push((recorded.to_owned(), host.to_owned()));
                self.paths.sort_by_key(|(r, _)| Reverse(r.len()));
                Ok(())
            }
        }
    }

    /// A recorded frame as it goes to the host: its paths and the id of a
    /// response are the host's.
    fn outgoing(&self, seq: u64, message: &Value) -> Result<Value, ReplayError> {
        let mut message = self.host_paths(message.clone());
        if message.get("method").is_none()
            && let Some(id) = message.get_mut("id")
        {
            let host = self.ids.get(&id.to_string());
            *id = host.cloned().ok_or(ReplayError::Unanswered {
                seq,
                id: id.clone(),
            })?;
        }
        Ok(message)
    }

    fn host_paths(&self, value: Value) -> Value {
        match value {
            Value::String(text) => {
                let replaced = self.paths.iter().fold(text, |t, (r, h)| t.replace(r, h));
                Value::String(replaced)
            }
            Value::Array(items) => items.into_iter().map(|v| self.host_paths(v)).collect(),
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(k, v)| (k, self.host_paths(v)))
                    .collect::<Map<_, _>>(),
            ),
            other => other,
        }
    }
}

/// Real time: the fake agent plays a real adapter as a separate process.
#[expect(
    clippy::disallowed_methods,
    reason = "the fake agent is an external process playing a real adapter in real time"
)]
fn now() -> Instant {
    Instant::now()
}

#[expect(
    clippy::disallowed_methods,
    reason = "the fake agent is an external process playing a real adapter in real time"
)]
async fn sleep_until(at: Instant) {
    tokio::time::sleep_until(at).await;
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn request(id: u64, method: &str, params: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    }

    /// The field where `host` differs from `recorded`, after `mapping` learned what it did.
    fn differs(mapping: &mut Mapping, recorded: &Value, host: &Value) -> Option<String> {
        mapping.check(recorded, host).err().map(|diff| diff.field)
    }

    #[test]
    fn unit_replay_rules_method_ids_paths_meta_and_prompt() {
        let mut m = Mapping::default();
        // The method.
        let new = |id, method: &str, cwd: &str| request(id, method, json!({"cwd": cwd}));
        let recorded = new(2, "session/new", "<tmp>/repos/claude");
        let wrong = new(7, "session/load", "/w");
        assert_eq!(
            differs(&mut m, &recorded, &wrong).as_deref(),
            Some("method")
        );
        // Ids: the host's request id stands for the recorded one; a
        // notification in a request's place does not match.
        assert_eq!(
            differs(&mut m, &recorded, &new(7, "session/new", "/w")),
            None
        );
        let response =
            json!({"jsonrpc": "2.0", "id": 2, "result": {"cwd": "<tmp>/repos/claude/a"}});
        let back = m.outgoing(9, &response).unwrap();
        assert_eq!(
            back,
            json!({"jsonrpc": "2.0", "id": 7, "result": {"cwd": "/w/a"}})
        );
        let mut notification = new(0, "session/new", "/w");
        notification.as_object_mut().unwrap().remove("id");
        assert_eq!(
            differs(
                &mut m,
                &new(3, "session/new", "<tmp>/repos/claude"),
                &notification
            )
            .as_deref(),
            Some("id")
        );
        let unknown = json!({"jsonrpc": "2.0", "id": 5, "result": {}});
        assert!(matches!(
            m.outgoing(9, &unknown),
            Err(ReplayError::Unanswered { seq: 9, .. })
        ));
        // Paths: a placeholder stands for the path the host sent first.
        assert_eq!(
            differs(&mut m, &recorded, &new(8, "session/new", "/w")),
            None
        );
        assert_eq!(
            differs(&mut m, &recorded, &new(8, "session/new", "/x")).as_deref(),
            Some("params.cwd")
        );
        // `_meta`: ignored at every level, on either side.
        let prompt = |text: &str, meta: Value| {
            let block = json!({"type": "text", "text": text, "_meta": meta});
            request(
                4,
                "session/prompt",
                json!({"sessionId": "s", "prompt": [block], "_meta": {"a": 1}}),
            )
        };
        assert_eq!(
            differs(
                &mut m,
                &prompt("hi", json!({"b": 2})),
                &prompt("hi", Value::Null)
            ),
            None
        );
        let mut extra = prompt("hi", json!(1));
        extra["params"]["meta"] = json!(1);
        assert_eq!(
            differs(&mut m, &prompt("hi", json!(1)), &extra).as_deref(),
            Some("params.meta")
        );
        // The prompt: exactly, down to a character.
        let diff = m
            .check(&prompt("hi", json!(1)), &prompt("hi!", json!(1)))
            .unwrap_err();
        assert_eq!(
            diff,
            Diff::new(
                "params.prompt[0].text",
                Some(&"hi".into()),
                Some(&"hi!".into())
            )
        );
        // Answers to the adapter's requests: its id, the recorded outcome.
        let answer = |id: u64, option: &str| json!({"jsonrpc": "2.0", "id": id, "result": {"outcome": {"outcome": "selected", "optionId": option}}});
        assert_eq!(
            differs(&mut m, &answer(0, "allow-once"), &answer(0, "allow-once")),
            None
        );
        assert_eq!(
            differs(&mut m, &answer(0, "allow-once"), &answer(1, "allow-once")).as_deref(),
            Some("id")
        );
        let field = differs(&mut m, &answer(0, "allow-once"), &answer(0, "reject"));
        assert_eq!(field.as_deref(), Some("result.outcome.optionId"));
    }
}
