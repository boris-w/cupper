//! The TS views are held to these (design 5.9.1, test #16): event bodies and
//! the view JSON Rust folds them into, which `@cupper/host-client` folds
//! too. Each case is `{name, view, ops, expected}`, `view` being `index` or
//! `session`.
//!
//! - UNIT-views-parity: `protocol/fixtures/views.json`, checked here on
//!   every PR; `UPDATE_GOLDEN=1` writes it again.
//! - UNIT-views-differential: random host streams, written nightly to the
//!   file `CUPPER_VIEW_CASES` names (`cargo xtask ci differential`).

use std::fs;
use std::path::PathBuf;

use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;
use serde::Serialize;
use serde_json::{Value, json};

use super::fixtures::*;
use super::props::{Step, host_run, script, steps};
use super::{IndexView, OpLimits, SessionView, View, Window};
use crate::event::{EventBody, StreamKind, Strictness};

fn case<V: View + Serialize>(name: &str, view: &str, ops: &[Value]) -> Value {
    let mut folded = V::default();
    for op in ops {
        let body = EventBody::decode(op, Strictness::Tolerant).unwrap().value;
        folded
            .apply(&body)
            .unwrap_or_else(|e| panic!("{name}: {op}: {e}"));
    }
    json!({"name": name, "view": view, "ops": ops, "expected": folded})
}

fn raw(ops: &[EventBody]) -> Vec<Value> {
    ops.iter()
        .map(|op| serde_json::to_value(op).unwrap())
        .collect()
}

/// Every sample of `events.json` that `V` takes, in file order. Each
/// `item.started` and `interaction.requested` gets an ID of its own, and the
/// events about an item or a request go to every one before them, so that
/// each kind of item meets each patch.
fn every_sample<V: View + Clone>(stream: StreamKind) -> Vec<Value> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../protocol/fixtures/events.json");
    let samples: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let (mut view, mut ops, mut items, mut requests) = (V::default(), vec![], vec![], vec![]);
    for (n, sample) in samples.iter().enumerate() {
        let mut op = sample["event"].clone();
        op.as_object_mut()
            .unwrap()
            .retain(|key, _| key == "type" || key == "data");
        let Ok(body) = EventBody::decode(&op, Strictness::Tolerant) else {
            continue;
        };
        if body.value.stream_kind().is_some_and(|kind| kind != stream) {
            continue;
        }
        let tag = body.value.tag();
        let candidates = match tag {
            _ if matches!(body.value, EventBody::Unknown(_)) => vec![op],
            "item.started" | "interaction.requested" => {
                let (key, ids) = match tag {
                    "item.started" => ("item_id", &mut items),
                    _ => ("request_id", &mut requests),
                };
                let id = format!("{}_{n}", &key[..1]);
                op["data"][key] = json!(id);
                ids.push(id);
                vec![op]
            }
            "item.delta" | "item.updated" | "item.completed" | "interaction.resolved" => {
                let (key, ids) = match tag {
                    "interaction.resolved" => ("request_id", &requests),
                    _ => ("item_id", &items),
                };
                let about = |id: &String| {
                    let mut about = op.clone();
                    about["data"][key] = json!(id);
                    about
                };
                ids.iter().map(about).collect()
            }
            _ => vec![op],
        };
        for candidate in candidates {
            let body = EventBody::decode(&candidate, Strictness::Tolerant)
                .unwrap()
                .value;
            let mut next = view.clone();
            if next.apply(&body).is_ok() {
                view = next;
                ops.push(candidate);
            }
        }
    }
    ops
}

/// A host stream three ways: the whole log; the snapshot of the host's
/// trimmed view; that snapshot with every older turn's history, newest first.
pub(super) fn streams(name: &str, steps: &[Step], window: Window) -> Vec<Value> {
    let ops = script(steps);
    let (host, saved, _) = host_run(&ops, &window);
    let snapshot = host.snapshot_ops_within(&OpLimits::V1);
    let first = host.turns.first().map_or(u32::MAX, |t| t.started.turn_no);
    let history = saved.range(..first).rev().flat_map(|(_, ops)| ops.clone());
    let with_history: Vec<EventBody> = snapshot.iter().cloned().chain(history).collect();
    vec![
        case::<SessionView>(&format!("{name}: the log"), "session", &raw(&ops)),
        case::<SessionView>(&format!("{name}: a snapshot"), "session", &raw(&snapshot)),
        case::<SessionView>(
            &format!("{name}: with history"),
            "session",
            &raw(&with_history),
        ),
    ]
}

/// The window's turns, a turn's history and the tail; a detached request;
/// late output; and two host streams.
fn scenarios() -> Vec<Value> {
    let (mut host, mut saved) = (SessionView::default(), vec![]);
    for n in 1..=4 {
        applied(
            &mut host,
            &[turn(n), item(&format!("i_loose{n}"), 0, "notice")],
        );
        applied(
            &mut host,
            &[item(&format!("i_{n}"), n, "agent_message"), end(n)],
        );
        saved.push(
            host.turn_ops(&format!("t_{n}").parse().unwrap(), &OpLimits::V1)
                .unwrap(),
        );
        host.trim(&Window {
            turns: 1,
            items: 100,
        });
    }
    let mut history = host.snapshot_ops();
    history.extend(saved[..3].iter().rev().flatten().cloned());
    history.extend([item("i_tail", 0, "notice"), turn(5)]);

    let mut asked = SessionView::default();
    let asks = [
        turn(1),
        ask("r_1", 1),
        ask("r_2", 1),
        answer("r_2", "resolved"),
        end(1),
    ];
    applied(&mut asked, &asks);
    let turn_1 = asked
        .turn_ops(&"t_1".parse().unwrap(), &OpLimits::V1)
        .unwrap();
    applied(&mut asked, &[turn(2), end(2)]);
    asked.trim(&Window {
        turns: 1,
        items: 100,
    });
    let mut detached = asked.snapshot_ops();
    detached.push(answer("r_1", "cancelled"));
    detached.extend(turn_1);

    let late_output = [
        turn(1),
        item("i_1", 1, "tool_call"),
        done(
            "i_1",
            "abandoned",
            json!({"may_still_run": true, "exit_code": 2, "params": {}}),
        ),
        end(1),
        turn(2),
        late("i_2", 2, "i_1", 1),
        delta("i_2", "output", "ready\n"),
        done(
            "i_2",
            "completed",
            json!({"exit_code": 0, "output_state": "complete", "title": "x"}),
        ),
    ];
    let mut cases = vec![
        case::<SessionView>(
            "history before the window and a tail",
            "session",
            &raw(&history),
        ),
        case::<SessionView>(
            "a detached request back in place",
            "session",
            &raw(&detached),
        ),
        case::<SessionView>(
            "late output of an abandoned tool call",
            "session",
            &raw(&late_output),
        ),
    ];
    // The first deterministic stream that finishes four turns, cut short.
    let mut runner = TestRunner::deterministic();
    loop {
        let steps = steps().new_tree(&mut runner).unwrap().current();
        let steps = &steps[..steps.len().min(70)];
        if script(steps)
            .iter()
            .filter(|op| op.tag() == "turn.completed")
            .count()
            >= 4
        {
            cases.extend(streams(
                "a host stream",
                steps,
                Window { turns: 1, items: 6 },
            ));
            break;
        }
    }
    cases
}

#[test]
fn unit_views_parity_fixture_is_what_the_rust_views_give() {
    let mut cases = vec![
        case::<IndexView>(
            "every index sample",
            "index",
            &every_sample::<IndexView>(StreamKind::Index),
        ),
        case::<SessionView>(
            "every session sample",
            "session",
            &every_sample::<SessionView>(StreamKind::Session),
        ),
    ];
    cases.extend(scenarios());
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../protocol/fixtures/views.json");
    let text = serde_json::to_string_pretty(&cases).unwrap() + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(&path, text).unwrap();
    }
    let committed: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        committed == Value::Array(cases),
        "protocol/fixtures/views.json is stale: run this test with UPDATE_GOLDEN=1"
    );
}

#[test]
#[ignore = "nightly: `cargo xtask ci differential` names the file in CUPPER_VIEW_CASES"]
fn unit_views_differential_writes_random_host_streams() {
    let path = std::env::var_os("CUPPER_VIEW_CASES").expect("CUPPER_VIEW_CASES names a file");
    let mut runner = TestRunner::default();
    let mut cases = Vec::new();
    for n in 0..300 {
        let steps = steps().new_tree(&mut runner).unwrap().current();
        let window = Window {
            turns: 1 + n % 3,
            items: n % 13,
        };
        cases.extend(streams(&format!("random {n}"), &steps, window));
    }
    fs::write(path, serde_json::to_vec(&cases).unwrap()).unwrap();
}
