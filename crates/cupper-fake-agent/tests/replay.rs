//! UNIT-replay over the fake agent's stdio pipes: the adapter's requests
//! wait for the host's answer, which must match; gaps are cut to 1/100; a
//! restarted agent plays the next segment; a mismatch names frame and field.

#[expect(dead_code, reason = "each test binary uses part of the shared client")]
mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{Fake, request, selected};
use cupper_fake_agent::replay::EXIT_MISMATCH;
use cupper_protocol::recording::{
    Body, Frame, Header, INSTANCE_EXITED, INSTANCE_STARTED, Recording, Source, Usage,
};
use serde_json::{Value, json};

fn note(event: &str, code: Value) -> Body {
    let note = json!({"event": event, "code": code});
    Body::Note(note.as_object().unwrap().clone())
}

/// A recording of `(instance, t, body)` frames, written to a fresh file.
fn recording(name: &str, frames: Vec<(u64, u64, Body)>) -> PathBuf {
    let header = Header {
        source: Source::Host,
        adapter: "fake".into(),
        adapter_version: "0".into(),
        cli: "fake".into(),
        cli_version: "0".into(),
        scenario: name.into(),
        usage: vec![Usage::Replay],
    };
    let frames = (1..).zip(frames);
    let frames = frames.map(|(seq, (instance, t, body))| Frame {
        seq,
        t,
        instance,
        body,
    });
    let recording = Recording {
        header,
        frames: frames.collect(),
    };
    let dir = std::env::temp_dir().join(format!("cupper-replay-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("recording.jsonl");
    std::fs::write(&path, recording.to_jsonl()).unwrap();
    path
}

/// A turn: initialize, session/new in `<tmp>/w`, a prompt that asks for a
/// permission (frame 9 is its answer) and ends 3 s after it.
fn turn() -> PathBuf {
    let send = |id: u64, method: &str, params: Value| {
        Body::Send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
    };
    let recv = |mut message: Value| {
        message["jsonrpc"] = json!("2.0");
        Body::Recv(message)
    };
    let client = json!({"name": "cupper", "version": "0.0.0"});
    let initialize = json!({"protocolVersion": 1, "clientCapabilities": {}, "clientInfo": client});
    let new = json!({"cwd": "<tmp>/w", "mcpServers": []});
    let prompt = json!({"sessionId": "s-1", "prompt": [{"type": "text", "text": "go"}]});
    let text = json!({"type": "text", "text": "in <tmp>/w"});
    let chunk = json!({"sessionUpdate": "agent_message_chunk", "content": text});
    let update =
        json!({"method": "session/update", "params": {"sessionId": "s-1", "update": chunk}});
    let options = json!([{"optionId": "allow-once"}, {"optionId": "reject"}]);
    let ask = json!({"sessionId": "s-1", "options": options});
    let ask = json!({"id": 0, "method": "session/request_permission", "params": ask});
    let answer = json!({"jsonrpc": "2.0", "id": 0, "result": selected("allow-once")});
    let frames = [
        (0, note(INSTANCE_STARTED, Value::Null)),
        (0, send(10, "initialize", initialize)),
        (5, recv(json!({"id": 10, "result": {"protocolVersion": 1}}))),
        (5, send(11, "session/new", new)),
        (9, recv(json!({"id": 11, "result": {"sessionId": "s-1"}}))),
        (100, send(12, "session/prompt", prompt)),
        (100, recv(update)),
        (200, recv(ask)),
        (300, Body::Send(answer)),
        (
            3300,
            recv(json!({"id": 12, "result": {"stopReason": "end_turn"}})),
        ),
        (3400, note(INSTANCE_EXITED, json!(0))),
    ];
    recording(
        "turn",
        frames.into_iter().map(|(t, body)| (0, t, body)).collect(),
    )
}

#[expect(
    clippy::disallowed_methods,
    reason = "measures the fake agent's real-time gaps"
)]
fn now() -> Instant {
    Instant::now()
}

#[test]
fn unit_replay_an_answer_waits_for_the_host_and_gaps_shrink_a_hundredfold() {
    let path = turn();
    let mut fake = Fake::spawn(&["--replay", path.to_str().unwrap()]);
    assert_eq!(fake.initialize()["result"], json!({"protocolVersion": 1}));
    assert_eq!(fake.open(Value::Null)["result"]["sessionId"], "s-1");
    let prompt = fake.start_prompt("go");
    let chunk = fake.wait_for("chunk", |m| m["method"] == "session/update");
    let text = format!("in {}", fake.cwd.display());
    assert_eq!(chunk["params"]["update"]["content"]["text"], text);
    let ask = fake.wait_for("permission", request("session/request_permission"));
    assert!(
        fake.quiet(200, |m| m["id"] == prompt),
        "the turn waits for the answer"
    );
    fake.write(json!({"jsonrpc": "2.0", "id": ask["id"], "result": selected("allow-once")}));
    let answered = now();
    assert_eq!(fake.response(prompt)["result"]["stopReason"], "end_turn");
    let gap = now() - answered;
    assert!(
        gap >= Duration::from_millis(30) && gap < Duration::from_millis(1500),
        "{gap:?}"
    );
    assert_eq!(fake.exit().0, Some(0));
}

#[test]
fn unit_replay_another_option_fails_naming_frame_and_field() {
    let path = turn();
    let mut fake = Fake::spawn(&["--replay", path.to_str().unwrap()]);
    fake.initialize();
    fake.open(Value::Null);
    fake.start_prompt("go");
    let ask = fake.wait_for("permission", request("session/request_permission"));
    fake.write(json!({"jsonrpc": "2.0", "id": ask["id"], "result": selected("reject")}));
    let (code, stderr) = fake.exit();
    assert_eq!(code, Some(EXIT_MISMATCH));
    let says = r#"frame 9: `result.outcome.optionId` is "reject", the recording has "allow-once""#;
    assert!(stderr.contains(says), "{stderr}");
}

#[test]
fn unit_replay_each_start_plays_the_next_segment() {
    let segment = |instance: u64, line: &str, code: u64| {
        [
            (instance, 0, note(INSTANCE_STARTED, Value::Null)),
            (instance, 10, Body::Stderr(line.to_owned())),
            (instance, 20, note(INSTANCE_EXITED, json!(code))),
        ]
    };
    let frames = segment(0, "first", 3)
        .into_iter()
        .chain(segment(1, "second", 0))
        .collect();
    let path = recording("segments", frames);
    let counter = path.with_file_name("segment");
    let args = [
        "--replay",
        path.to_str().unwrap(),
        "--segment-file",
        counter.to_str().unwrap(),
    ];
    let (code, stderr) = Fake::spawn(&args).exit();
    assert_eq!((code, stderr.trim()), (Some(3), "first"));
    let (code, stderr) = Fake::spawn(&args).exit();
    assert_eq!((code, stderr.trim()), (Some(0), "second"));
    let (code, stderr) = Fake::spawn(&args).exit();
    assert!(
        code == Some(1) && stderr.contains("2 segments, so no segment 2"),
        "{stderr}"
    );
}
