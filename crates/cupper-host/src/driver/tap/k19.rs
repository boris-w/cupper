//! The tolerant rewrite of the SDK's closed enums (K19, design 5.6.10).
//!
//! The SDK fails a whole message whose `stopReason` (prompt response) or
//! permission option `kind` holds a value it does not know, and quietly turns
//! an unknown tool call `status` into a default. The tap replaces such a value
//! with one the SDK parses and keeps what the agent sent under
//! `_meta.cupper.original.<field>` of the same object, where [`original`]
//! finds it. The stand-in only lets the SDK parse the message: the extraction
//! layer decides on the original, the protocol carries the original, and an
//! unknown option kind counts as the strictest (security #5, design 5.6.4).

use agent_client_protocol::schema::v1::{Meta, PermissionOptionKind, StopReason, ToolCallStatus};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// Whether `line` may hold one of the fields; only such lines are parsed.
pub(super) fn may_apply(line: &str) -> bool {
    [
        "\"stopReason\"",
        "\"session/request_permission\"",
        "\"tool_call",
    ]
    .iter()
    .any(|key| line.contains(key))
}

/// Rewrites every unknown value in `msg`, a message or a batch; returns
/// whether anything changed.
pub(super) fn rewrite(msg: &mut Value) -> bool {
    match msg {
        Value::Array(batch) => batch
            .iter_mut()
            .fold(false, |changed, msg| rewrite_one(msg) | changed),
        msg => rewrite_one(msg),
    }
}

/// What the agent sent for `field` of the object that carries `meta`, when the
/// tap rewrote it.
pub fn original<'a>(meta: Option<&'a Meta>, field: &str) -> Option<&'a Value> {
    meta?.get("cupper")?.get("original")?.get(field)
}

fn rewrite_one(msg: &mut Value) -> bool {
    let method = msg
        .get("method")
        .map(|method| method.as_str().unwrap_or_default().to_owned());
    match method.as_deref() {
        None => msg
            .get_mut("result")
            .is_some_and(|result| fix::<StopReason>(result, "stopReason", "end_turn")),
        Some("session/request_permission") => {
            let Some(params) = msg.get_mut("params") else {
                return false;
            };
            let mut changed = params
                .get_mut("toolCall")
                .is_some_and(|tool_call| fix::<ToolCallStatus>(tool_call, "status", "in_progress"));
            for option in params
                .get_mut("options")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                changed |= fix::<PermissionOptionKind>(option, "kind", "reject_once");
            }
            changed
        }
        Some("session/update") => msg.pointer_mut("/params/update").is_some_and(|update| {
            let kind = update.get("sessionUpdate").and_then(Value::as_str);
            matches!(kind, Some("tool_call" | "tool_call_update"))
                && fix::<ToolCallStatus>(update, "status", "in_progress")
        }),
        Some(_) => false,
    }
}

/// Replaces `obj[field]` with `fallback` when the SDK cannot read it as an
/// `E`, keeping the original in the object's `_meta`.
fn fix<E: DeserializeOwned>(obj: &mut Value, field: &str, fallback: &str) -> bool {
    let Some(map) = obj.as_object_mut() else {
        return false;
    };
    match map.get(field) {
        Some(value) if !value.is_null() && E::deserialize(value).is_err() => {}
        _ => return false,
    }
    let original = map
        .insert(field.to_owned(), fallback.into())
        .unwrap_or_default();
    let meta = object_at(map, "_meta");
    let cupper = object_at(meta, "cupper");
    object_at(cupper, "original").insert(field.to_owned(), original);
    true
}

/// The object under `key`, made one if it is missing or something else.
fn object_at<'a>(map: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let slot = map.entry(key).or_insert_with(|| Value::Object(Map::new()));
    if !slot.is_object() {
        *slot = Value::Object(Map::new());
    }
    slot.as_object_mut()
        .expect("the slot was just made an object")
}
