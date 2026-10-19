//! Property tests over session streams written the way the host writes them
//! (ADR-04, test #22): every open item and pending request ends with its
//! turn, late output lands in a later turn, outside a turn there are only
//! notices and requests.

use std::collections::BTreeMap;

use proptest::prelude::*;
use serde_json::json;

use super::fixtures::*;
use super::{OpLimits, SessionView, View, Window, paginate};
use crate::annotations::TimestampMs;
use crate::coalesce::coalesce;
use crate::envelope::Event;
use crate::event::EventBody;
use crate::sync::StreamCursor;
use crate::text::escaped_len;

#[derive(Debug, Clone)]
enum Step {
    /// Start a turn, or end the running one.
    Turn,
    /// Start an item; without `turn_id` when the flag is set.
    Item(u8, bool),
    /// Append to an open item's text, or its output when the flag is set.
    Delta(u8, bool, String),
    Update(u8, String),
    Done(u8),
    /// Ask; outside any turn when the flag is set.
    Ask(bool),
    Answer(u8),
    /// Late output of a tool call abandoned in an earlier turn.
    Late(u8),
    State(u8),
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    let text = || "\\PC{0,12}|[\"\\\\\\n\\x01é]{0,6}";
    let step = prop_oneof![
        2 => Just(Step::Turn),
        4 => (any::<u8>(), any::<bool>()).prop_map(|(k, l)| Step::Item(k, l)),
        6 => (any::<u8>(), any::<bool>(), text()).prop_map(|(p, o, t)| Step::Delta(p, o, t)),
        1 => (any::<u8>(), text()).prop_map(|(p, t)| Step::Update(p, t)),
        2 => any::<u8>().prop_map(Step::Done),
        1 => any::<bool>().prop_map(Step::Ask),
        1 => any::<u8>().prop_map(Step::Answer),
        1 => any::<u8>().prop_map(Step::Late),
        1 => any::<u8>().prop_map(Step::State),
    ];
    prop::collection::vec(step, 0..150)
}

/// The host's side of a session: what is open, pending and abandoned.
#[derive(Default)]
struct Host {
    ops: Vec<EventBody>,
    ids: u32,
    turn_no: u32,
    running: bool,
    /// Open items, and whether each is a tool call.
    open: Vec<(String, bool)>,
    pending: Vec<String>,
    abandoned: Vec<(String, u32)>,
}

fn pick<T>(list: &mut Vec<T>, n: u8) -> Option<T> {
    (!list.is_empty()).then(|| list.remove(usize::from(n) % list.len()))
}

fn nth<T>(list: &[T], n: u8) -> Option<&T> {
    list.get(usize::from(n) % list.len().max(1))
}

impl Host {
    fn id(&mut self, prefix: &str) -> String {
        self.ids += 1;
        format!("{prefix}_{}", self.ids)
    }

    fn step(&mut self, step: &Step) {
        let turn_no = self.turn_no;
        let current = if self.running { turn_no } else { 0 };
        let op = match step {
            Step::Turn if self.running => {
                for (id, tool) in std::mem::take(&mut self.open) {
                    self.ops.push(done(
                        &id,
                        if tool { "abandoned" } else { "completed" },
                        json!({}),
                    ));
                    if tool {
                        self.abandoned.push((id, turn_no));
                    }
                }
                for id in std::mem::take(&mut self.pending) {
                    self.ops.push(answer(&id, "cancelled"));
                }
                self.running = false;
                end(turn_no)
            }
            Step::Turn => {
                (self.turn_no, self.running) = (turn_no + 1, true);
                turn(turn_no + 1)
            }
            Step::Item(kind, loose) => {
                let kinds = ["agent_message", "thought", "tool_call", "notice"];
                let kind = if self.running {
                    kinds[usize::from(*kind) % 4]
                } else {
                    "notice"
                };
                let id = self.id("i");
                let op = item(&id, if *loose { 0 } else { current }, kind);
                if kind != "notice" {
                    self.open.push((id, kind == "tool_call"));
                }
                op
            }
            Step::Delta(n, output, text) => match nth(&self.open, *n) {
                Some((id, _)) => delta(id, if *output { "output" } else { "text" }, text),
                None => return,
            },
            Step::Update(n, text) => match nth(&self.open, *n) {
                Some((id, _)) => body(json!({"type": "item.updated", "data": {"item_id": id,
                    "title": text, "output_state": "truncated", "output_tail": text,
                    "output_bytes": text.len()}})),
                None => return,
            },
            Step::Done(n) => {
                let Some((id, tool)) = pick(&mut self.open, *n) else {
                    return;
                };
                let output = json!({"exit_code": 0, "output_state": "complete",
                    "output": {"type": "text", "text": "ok"}});
                done(&id, "completed", if tool { output } else { json!({}) })
            }
            Step::Ask(loose) => {
                let id = self.id("r");
                let op = ask(&id, if *loose { 0 } else { current });
                self.pending.push(id);
                op
            }
            Step::Answer(n) => match pick(&mut self.pending, *n) {
                Some(id) => answer(&id, "resolved"),
                None => return,
            },
            Step::Late(n) => {
                let Some((of, of_turn)) = self
                    .running
                    .then(|| pick(&mut self.abandoned, *n))
                    .flatten()
                else {
                    return;
                };
                let id = self.id("i");
                self.open.push((id.clone(), false));
                late(&id, turn_no, &of, of_turn)
            }
            Step::State(n) => body(match n % 5 {
                0 => {
                    json!({"type": "session.configured", "data": {"agent": "codex", "read_only": false,
                    "workspace": {"kind": "local", "path": "/w"}, "config": {"model": format!("m{n}")},
                    "project_config": "loaded"}})
                }
                1 => json!({"type": "queue.updated", "data": {"version": n, "entries": []}}),
                2 => {
                    json!({"type": "workspace.preparing", "data": {"phase": "checkout", "progress": n % 101}})
                }
                3 if self.running => {
                    json!({"type": "snapshot.recorded", "data": {"turn_id": format!("t_{turn_no}"),
                    "phase": if n % 2 == 0 { "start" } else { "end" }, "status": "ok", "excluded": []}})
                }
                _ => {
                    json!({"type": "usage.updated", "data": {"context_used": n, "context_size": 1000}})
                }
            }),
        };
        self.ops.push(op);
    }
}

fn script(steps: &[Step]) -> Vec<EventBody> {
    let mut host = Host::default();
    for step in steps {
        host.step(step);
    }
    host.ops
}

/// The host's view, trimmed to `window` as each turn ends, with each turn's
/// ops saved at its end; and the view of the whole log.
fn host_run(
    ops: &[EventBody],
    window: &Window,
) -> (SessionView, BTreeMap<u32, Vec<EventBody>>, SessionView) {
    let (mut host, mut replay) = (SessionView::default(), SessionView::default());
    let mut saved = BTreeMap::new();
    for op in ops {
        applied(&mut host, std::slice::from_ref(op));
        applied(&mut replay, std::slice::from_ref(op));
        if let EventBody::TurnCompleted(t) = op {
            let turn_ops = host.turn_ops(&t.turn_id, &OpLimits::V1).unwrap();
            saved.insert(host.turns.last().unwrap().started.turn_no, turn_ops);
            host.trim(window);
        }
    }
    (host, saved, replay)
}

fn paged(view: &mut SessionView, ops: &[EventBody], page_bytes: usize) {
    for page in paginate(ops, page_bytes) {
        applied(view, &ops[page]);
    }
}

proptest! {
    /// Send-time coalescing merges `item.delta`s; folding the merged events
    /// gives the same view as folding them one by one.
    #[test]
    fn prop_view_coalesce_merged_events_apply_like_single_ones(steps in steps(), cap in 6usize..200) {
        let events: Vec<Event<EventBody>> = script(&steps).into_iter().zip(1..).map(|(body, seq)| {
            Event { seq, from: None, v: 1, ts: TimestampMs(seq), cause: None, fallback: None, body }
        }).collect();
        let fold = |events: &[Event<EventBody>]| {
            let (mut view, mut cursor) = (SessionView::default(), StreamCursor::at(0));
            for e in events {
                cursor.accept(e).unwrap();
                view.apply(&e.body).unwrap();
            }
            view
        };
        let merged = coalesce(events.clone(), cap);
        prop_assert_eq!(fold(&merged), fold(&events));
    }

    /// A snapshot cut into small deltas, split items and small pages
    /// rebuilds the same view as one cut at the protocol's limits.
    #[test]
    fn prop_view_coalesce_split_snapshot_applies_like_unsplit(
        steps in steps(), delta in 6usize..40, op in 100usize..600, page_bytes in 1usize..3_000,
    ) {
        let (view, _, _) = host_run(&script(&steps), &Window { turns: 2, items: 6 });
        let ops = view.snapshot_ops_within(&OpLimits { delta, op });
        for o in &ops {
            if let EventBody::ItemDelta(d) = o {
                prop_assert!(escaped_len(&d.text) <= delta);
            }
        }
        let mut split = SessionView::default();
        paged(&mut split, &ops, page_bytes);
        prop_assert_eq!(&split, &view);
        prop_assert_eq!(&rebuilt(&view, &OpLimits::V1), &view);
        let json = serde_json::to_value(&view).unwrap();
        prop_assert_eq!(&serde_json::from_value::<SessionView>(json).unwrap(), &view);
    }

    /// The window's snapshot plus every turn's history, newest first and in
    /// pages, is the view of the whole log.
    #[test]
    fn unit_view_window_snapshot_and_history_replay_the_log(
        steps in steps(), turns in 1usize..4, items in 0usize..12, page_bytes in 1usize..3_000,
    ) {
        let (host, saved, replay) = host_run(&script(&steps), &Window { turns, items });
        let mut client = rebuilt(&host, &OpLimits::V1);
        let first = client.turns.first().map_or(u32::MAX, |t| t.started.turn_no);
        for ops in saved.range(..first).rev().map(|(_, ops)| ops) {
            paged(&mut client, ops, page_bytes);
        }
        prop_assert_eq!(client, replay);
    }
}
