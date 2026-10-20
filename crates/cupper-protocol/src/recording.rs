//! Recording format v1 (design 5.9.3): the ACP frames between an adapter and
//! its client, as JSONL. The host's `diag/recording_export` writes it, the
//! fake agent replays it and the golden tests read it. Parsing and writing
//! work on text only; callers read and write the files. Not part of the
//! exported schema.
//!
//! The first line is the [`Header`]; every other line is a [`Frame`]. A
//! recording is cut into segments, one per adapter instance: instance `n`
//! (counted from 0, like the fake agent's `--segment`) begins with an
//! [`INSTANCE_STARTED`] note, and within it `seq` increases and `t`, the
//! milliseconds since the segment began, never goes back.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::limits::MAX_SAFE_INTEGER;

pub const FORMAT: &str = "cupper-rec";
pub const VERSION: u64 = 1;
/// The note event that begins a segment.
pub const INSTANCE_STARTED: &str = "instance_started";
/// The note event for the end of the adapter process.
pub const INSTANCE_EXITED: &str = "instance_exited";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Converted once from the SP-1 recordings; extraction goldens only.
    Sp1,
    /// Driven by the host (`cargo xtask compat`).
    Host,
}

/// What a recording is used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Usage {
    GoldA,
    GoldB,
    Replay,
}

/// The first line, without `format` and `v`, which [`Header::to_line`] adds
/// and parsing checks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub source: Source,
    pub adapter: String,
    pub adapter_version: String,
    pub cli: String,
    pub cli_version: String,
    pub scenario: String,
    pub usage: Vec<Usage>,
}

impl Header {
    pub fn to_line(&self) -> String {
        #[derive(Serialize)]
        struct Line<'a> {
            format: &'static str,
            v: u64,
            #[serde(flatten)]
            header: &'a Header,
        }
        let line = Line {
            format: FORMAT,
            v: VERSION,
            header: self,
        };
        serde_json::to_string(&line).expect("a header serializes")
    }

    fn from_line(line: &str) -> Result<Self, FormatErrorKind> {
        let mut map: Map<String, Value> =
            serde_json::from_str(line).map_err(|e| FormatErrorKind::Header(e.to_string()))?;
        if map.remove("format") != Some(Value::from(FORMAT)) {
            return Err(FormatErrorKind::NotRecording);
        }
        match map.remove("v") {
            Some(v) if v == VERSION => {}
            v => return Err(FormatErrorKind::Version(v.unwrap_or_default().to_string())),
        }
        serde_json::from_value(Value::Object(map))
            .map_err(|e| FormatErrorKind::Header(e.to_string()))
    }
}

/// Which way a frame went, from the adapter's client's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    Send,
    Recv,
    RecvNonjson,
    Stderr,
    Note,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub seq: u64,
    /// Milliseconds since the segment began.
    pub t: u64,
    pub instance: u64,
    pub body: Body,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// A JSON-RPC message to the adapter (`msg`).
    Send(Value),
    /// A JSON-RPC message from the adapter (`msg`).
    Recv(Value),
    /// A stdout line that is not JSON (`data`).
    RecvNonjson(String),
    /// A stderr line (`data`).
    Stderr(String),
    /// The recorder's own note (`note`): `event` is a string, the rest is free.
    Note(Map<String, Value>),
}

impl Body {
    pub fn dir(&self) -> Dir {
        match self {
            Body::Send(_) => Dir::Send,
            Body::Recv(_) => Dir::Recv,
            Body::RecvNonjson(_) => Dir::RecvNonjson,
            Body::Stderr(_) => Dir::Stderr,
            Body::Note(_) => Dir::Note,
        }
    }

    /// A note's `event`.
    pub fn event(&self) -> Option<&str> {
        match self {
            Body::Note(note) => note.get("event").and_then(Value::as_str),
            _ => None,
        }
    }
}

/// A frame line as it is written.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Line {
    seq: u64,
    t: u64,
    instance: u64,
    dir: Dir,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    msg: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<Map<String, Value>>,
}

impl Frame {
    pub fn to_line(&self) -> String {
        let mut line = Line {
            seq: self.seq,
            t: self.t,
            instance: self.instance,
            dir: self.body.dir(),
            msg: None,
            data: None,
            note: None,
        };
        match &self.body {
            Body::Send(msg) | Body::Recv(msg) => line.msg = Some(msg.clone()),
            Body::RecvNonjson(data) | Body::Stderr(data) => line.data = Some(data.clone()),
            Body::Note(note) => line.note = Some(note.clone()),
        }
        serde_json::to_string(&line).expect("a frame serializes")
    }

    fn from_line(text: &str) -> Result<Self, FormatErrorKind> {
        let line: Line =
            serde_json::from_str(text).map_err(|e| FormatErrorKind::Frame(e.to_string()))?;
        for (field, value) in [
            ("seq", line.seq),
            ("t", line.t),
            ("instance", line.instance),
        ] {
            if value > MAX_SAFE_INTEGER {
                return Err(FormatErrorKind::TooLarge(field));
            }
        }
        let body = match (line.dir, line.msg, line.data, line.note) {
            (Dir::Send, Some(msg), None, None) => Body::Send(msg),
            (Dir::Recv, Some(msg), None, None) => Body::Recv(msg),
            (Dir::RecvNonjson, None, Some(data), None) => Body::RecvNonjson(data),
            (Dir::Stderr, None, Some(data), None) => Body::Stderr(data),
            (Dir::Note, None, None, Some(note))
                if note.get("event").is_some_and(Value::is_string) =>
            {
                Body::Note(note)
            }
            (dir, ..) => return Err(FormatErrorKind::Payload(dir)),
        };
        Ok(Frame {
            seq: line.seq,
            t: line.t,
            instance: line.instance,
            body,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recording {
    pub header: Header,
    pub frames: Vec<Frame>,
}

impl Recording {
    /// Parses and checks a whole recording; an error names its line (from 1).
    pub fn parse(text: &str) -> Result<Self, FormatError> {
        let mut lines = (1..).zip(text.lines());
        let (_, first) = lines.next().ok_or(FormatError {
            line: 1,
            kind: FormatErrorKind::Empty,
        })?;
        let header = Header::from_line(first).map_err(|kind| FormatError { line: 1, kind })?;
        let mut frames: Vec<Frame> = Vec::new();
        for (number, text) in lines {
            let frame = Frame::from_line(text)
                .and_then(|frame| follows(frames.last(), &frame).map(|()| frame))
                .map_err(|kind| FormatError { line: number, kind })?;
            frames.push(frame);
        }
        Ok(Recording { header, frames })
    }

    /// The recording as JSONL, one line per header and frame.
    pub fn to_jsonl(&self) -> String {
        let lines =
            std::iter::once(self.header.to_line()).chain(self.frames.iter().map(Frame::to_line));
        lines.map(|line| line + "\n").collect()
    }

    /// The frames of each adapter instance, in order.
    pub fn segments(&self) -> impl Iterator<Item = &[Frame]> {
        self.frames.chunk_by(|a, b| a.instance == b.instance)
    }
}

/// Whether `frame` may follow `last`: the segment rules.
fn follows(last: Option<&Frame>, frame: &Frame) -> Result<(), FormatErrorKind> {
    let starts = frame.body.event() == Some(INSTANCE_STARTED);
    let instance = frame.instance;
    let next = last.map_or(0, |last| last.instance + 1);
    match last {
        Some(last) if instance == last.instance => {
            if starts {
                Err(FormatErrorKind::StartedMidSegment)
            } else if frame.seq <= last.seq {
                Err(FormatErrorKind::Seq {
                    seq: frame.seq,
                    last: last.seq,
                })
            } else if frame.t < last.t {
                Err(FormatErrorKind::Time {
                    t: frame.t,
                    last: last.t,
                })
            } else {
                Ok(())
            }
        }
        _ if instance == next && starts => Ok(()),
        _ if instance == next => Err(FormatErrorKind::SegmentStart(instance)),
        _ => Err(FormatErrorKind::Instance {
            found: instance,
            expected: next,
        }),
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
#[error("line {line}: {kind}")]
pub struct FormatError {
    /// From 1; the header is line 1.
    pub line: usize,
    pub kind: FormatErrorKind,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum FormatErrorKind {
    #[error("the recording is empty")]
    Empty,
    #[error("not a recording: the header needs `\"format\": \"{FORMAT}\"`")]
    NotRecording,
    #[error("format version {0} is not supported; this reader knows v{VERSION}")]
    Version(String),
    #[error("bad header: {0}")]
    Header(String),
    #[error("bad frame: {0}")]
    Frame(String),
    #[error("`{0}` is above 2^53 - 1")]
    TooLarge(&'static str),
    #[error("a `{}` frame carries {} and nothing else", dir_name(*.0), payload_name(*.0))]
    Payload(Dir),
    #[error("instance {found} where instance {expected} was due")]
    Instance { found: u64, expected: u64 },
    #[error("segment {0} does not begin with an `{INSTANCE_STARTED}` note")]
    SegmentStart(u64),
    #[error("`{INSTANCE_STARTED}` in the middle of a segment")]
    StartedMidSegment,
    #[error("seq {seq} does not increase from {last}")]
    Seq { seq: u64, last: u64 },
    #[error("t {t} goes back from {last}")]
    Time { t: u64, last: u64 },
}

fn dir_name(dir: Dir) -> &'static str {
    match dir {
        Dir::Send => "send",
        Dir::Recv => "recv",
        Dir::RecvNonjson => "recv_nonjson",
        Dir::Stderr => "stderr",
        Dir::Note => "note",
    }
}

fn payload_name(dir: Dir) -> &'static str {
    match dir {
        Dir::Send | Dir::Recv => "`msg`",
        Dir::RecvNonjson | Dir::Stderr => "a string `data`",
        Dir::Note => "a `note` object with a string `event`",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const HEADER: &str = r#"{"format":"cupper-rec","v":1,"source":"host","adapter":"codex-acp","adapter_version":"2.2.2","cli":"codex","cli_version":"0.160.1","scenario":"s09-load-resume","usage":["gold-a","gold-b","replay"]}"#;

    /// A recording with every `dir` and two segments, line by line.
    fn sample() -> Vec<String> {
        let started = r#""dir":"note","note":{"event":"instance_started"}"#;
        [
            HEADER.into(),
            format!(r#"{{"seq":1,"t":0,"instance":0,{started}}}"#),
            r#"{"seq":2,"t":1,"instance":0,"dir":"send","msg":{"jsonrpc":"2.0","id":1,"method":"initialize"}}"#.into(),
            r#"{"seq":3,"t":170,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","id":1,"result":{}}}"#.into(),
            r#"{"seq":4,"t":170,"instance":0,"dir":"recv_nonjson","data":"warming up"}"#.into(),
            r#"{"seq":5,"t":171,"instance":0,"dir":"stderr","data":"debug: ready"}"#.into(),
            r#"{"seq":6,"t":900,"instance":0,"dir":"note","note":{"event":"instance_exited","code":0}}"#.into(),
            format!(r#"{{"seq":1,"t":0,"instance":1,{started}}}"#),
            r#"{"seq":2,"t":3,"instance":1,"dir":"send","msg":{"jsonrpc":"2.0","id":1,"method":"session/resume"}}"#.into(),
        ]
        .into()
    }

    fn text(lines: &[String]) -> String {
        lines.iter().map(|line| format!("{line}\n")).collect()
    }

    #[test]
    fn unit_rec_format_round_trips_every_dir_and_segment() {
        let recording = Recording::parse(&text(&sample())).expect("valid");
        assert_eq!(recording.header.source, Source::Host);
        assert_eq!(
            recording.header.usage,
            [Usage::GoldA, Usage::GoldB, Usage::Replay]
        );
        let dirs: Vec<Dir> = recording.frames.iter().map(|f| f.body.dir()).collect();
        use Dir::*;
        assert_eq!(
            dirs,
            [Note, Send, Recv, RecvNonjson, Stderr, Note, Note, Send]
        );
        assert_eq!(
            recording.frames[3].body,
            Body::RecvNonjson("warming up".into())
        );
        assert_eq!(recording.frames[5].body.event(), Some(INSTANCE_EXITED));
        let segments: Vec<usize> = recording.segments().map(<[Frame]>::len).collect();
        assert_eq!(segments, [6, 2]);

        let written = recording.to_jsonl();
        assert!(written.starts_with(r#"{"format":"cupper-rec","v":1,"source":"host","#));
        assert_eq!(Recording::parse(&written), Ok(recording.clone()));
        let msg = &recording.frames[1].body;
        assert_eq!(
            *msg,
            Body::Send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}))
        );
    }

    /// One case per line: `<line> <replacement> => <start of the error message>`.
    /// Line 1 is the header; the replacement `-` stands for an empty line.
    const ERRORS: &str = r#"
1 - => line 1: the recording is empty
1 {} => line 1: not a recording: the header needs `"format": "cupper-rec"`
1 {"format":"cupper-rec","v":2} => line 1: format version 2 is not supported; this reader knows v1
1 {"format":"cupper-rec","v":1,"source":"sp2"} => line 1: bad header: unknown variant `sp2`
1 {"format":"cupper-rec","v":1,"source":"sp1"} => line 1: bad header: missing field `adapter`
2 {"seq":1,"t":0,"instance":0,"dir":"send","msg":{}} => line 2: segment 0 does not begin with an `instance_started` note
3 <not json> => line 3: bad frame: expected value
3 {"seq":2,"t":1,"instance":0,"dir":"sent","msg":{}} => line 3: bad frame: unknown variant `sent`
3 {"seq":2,"t":1,"instance":0,"dir":"send","msg":{},"x":1} => line 3: bad frame: unknown field `x`
3 {"seq":2,"t":-1,"instance":0,"dir":"send","msg":{}} => line 3: bad frame: invalid value: integer `-1`
3 - => line 3: bad frame: EOF
4 {"seq":3,"t":170,"instance":0,"dir":"recv","data":"x"} => line 4: a `recv` frame carries `msg` and nothing else
4 {"seq":3,"t":170,"instance":0,"dir":"recv"} => line 4: a `recv` frame carries `msg` and nothing else
5 {"seq":4,"t":170,"instance":0,"dir":"recv_nonjson","msg":{}} => line 5: a `recv_nonjson` frame carries a string `data` and nothing else
6 {"seq":5,"t":171,"instance":0,"dir":"stderr","data":"x","msg":{}} => line 6: a `stderr` frame carries a string `data` and nothing else
7 {"seq":6,"t":900,"instance":0,"dir":"note","note":{"code":0}} => line 7: a `note` frame carries a `note` object with a string `event` and nothing else
6 {"seq":5,"t":171,"instance":0,"dir":"note","note":{"event":"instance_started"}} => line 6: `instance_started` in the middle of a segment
6 {"seq":4,"t":171,"instance":0,"dir":"stderr","data":"x"} => line 6: seq 4 does not increase from 4
6 {"seq":5,"t":169,"instance":0,"dir":"stderr","data":"x"} => line 6: t 169 goes back from 170
6 {"seq":9007199254740992,"t":171,"instance":0,"dir":"stderr","data":"x"} => line 6: `seq` is above 2^53 - 1
8 {"seq":1,"t":0,"instance":2,"dir":"note","note":{"event":"instance_started"}} => line 8: instance 2 where instance 1 was due
9 {"seq":2,"t":3,"instance":2,"dir":"send","msg":{}} => line 9: segment 2 does not begin with an `instance_started` note
9 {"seq":2,"t":3,"instance":3,"dir":"send","msg":{}} => line 9: instance 3 where instance 2 was due
"#;

    #[test]
    fn unit_rec_format_reports_errors_with_their_line_and_reason() {
        for case in ERRORS.trim().lines() {
            let (line, rest) = case.split_once(' ').expect("a line number");
            let (replacement, message) = rest.split_once(" => ").expect("a message");
            let mut lines = sample();
            let line: usize = line.parse().expect("a line number");
            lines[line - 1] = replacement.trim_start_matches('-').to_owned();
            // Without a header the whole recording is empty.
            let text = if lines[0].is_empty() {
                String::new()
            } else {
                text(&lines)
            };
            let error = Recording::parse(&text).expect_err(case).to_string();
            assert!(error.starts_with(message), "{case}\n  got: {error}");
        }
    }
}
