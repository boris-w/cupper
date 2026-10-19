//! Session-stream bodies for the view tests. Turn `n` is `t_n`; `turn: 0`
//! leaves `turn_id` out.

use serde_json::{Value, json};

use super::{OpLimits, SessionView, View};
use crate::event::{EventBody, Strictness};

pub fn body(raw: Value) -> EventBody {
    EventBody::decode(&raw, Strictness::Strict)
        .unwrap_or_else(|e| panic!("{raw}: {e}"))
        .value
}

fn data(kind: &str, data: Value) -> EventBody {
    body(json!({"type": kind, "data": data}))
}

fn with_turn(mut data: Value, turn: u32) -> Value {
    if turn > 0 {
        data["turn_id"] = json!(format!("t_{turn}"));
    }
    data
}

pub fn turn(n: u32) -> EventBody {
    data(
        "turn.started",
        json!({"turn_id": format!("t_{n}"), "turn_no": n,
        "origin": "user", "started_at": n}),
    )
}

pub fn end(n: u32) -> EventBody {
    data(
        "turn.completed",
        json!({"turn_id": format!("t_{n}"), "status": "completed",
        "ended_at": n, "duration_ms": 1, "final_item_ids": []}),
    )
}

/// An item of `kind`: `agent_message`, `thought`, `notice`, or `tool_call`
/// (an `execute` call).
pub fn item(id: &str, turn: u32, kind: &str) -> EventBody {
    let mut started = json!({"item_id": id, "started_at": 1, "kind": kind});
    if kind == "tool_call" {
        let tool = json!({"tool_kind": "execute", "title": "Run", "paths": [], "command": "npm start",
            "cwd": "/w", "output_state": "none", "extraction": "full"});
        started
            .as_object_mut()
            .unwrap()
            .extend(tool.as_object().unwrap().clone());
    } else if kind == "notice" {
        started["code"] = json!("session_resumed");
        started["params"] = json!({});
    }
    data("item.started", with_turn(started, turn))
}

pub fn delta(id: &str, field: &str, text: &str) -> EventBody {
    data(
        "item.delta",
        json!({"item_id": id, "field": field, "text": text}),
    )
}

/// `item.completed` with `patch` as its final fields.
pub fn done(id: &str, status: &str, patch: Value) -> EventBody {
    let mut end = json!({"item_id": id, "status": status, "ended_at": 2, "duration_ms": 1});
    end.as_object_mut()
        .unwrap()
        .extend(patch.as_object().unwrap().clone());
    data("item.completed", end)
}

/// A `late_tool_output` item for the tool call `of` in turn `of_turn`.
pub fn late(id: &str, turn: u32, of: &str, of_turn: u32) -> EventBody {
    let started = json!({"item_id": id, "started_at": 3, "kind": "late_tool_output",
        "ref": {"item_id": of, "turn_id": format!("t_{of_turn}"), "title": "Run", "command": "npm start"},
        "output_state": "streaming"});
    data("item.started", with_turn(started, turn))
}

pub fn ask(id: &str, turn: u32) -> EventBody {
    let request = json!({"request_id": id, "created_at": 1, "digest": format!("d_{}", "0".repeat(64)),
        "actionable_by": ["desktop_ui"], "request": {"type": "auth_url", "url": "https://example.com",
        "message": "Sign in"}});
    data("interaction.requested", with_turn(request, turn))
}

pub fn answer(id: &str, status: &str) -> EventBody {
    data(
        "interaction.resolved",
        json!({"request_id": id, "status": status}),
    )
}

/// Applies every op, panicking on a view error.
pub fn applied(view: &mut SessionView, ops: &[EventBody]) {
    for op in ops {
        view.apply(op).unwrap_or_else(|e| panic!("{op:?}: {e}"));
    }
}

/// A fresh view from `view`'s snapshot.
pub fn rebuilt(view: &SessionView, limits: &OpLimits) -> SessionView {
    let mut fresh = SessionView::default();
    applied(&mut fresh, &view.snapshot_ops_within(limits));
    fresh
}
