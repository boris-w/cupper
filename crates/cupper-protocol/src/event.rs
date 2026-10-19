//! The event catalog (design 5.3.5): [`EventBody`] holds the `type` and
//! `data` of every event of both streams, and a reader decodes it either
//! strictly or tolerantly (design 5.3.11). Only `item.delta` is mergeable.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::capabilities::SessionCapabilities;
use crate::coalesce::Mergeable;
use crate::envelope::{Event, UnknownEvent};
use crate::index::{
    AgentsUpdated, ClientsUpdated, ElevationRequested, ElevationResolved, HostUpdated,
    NotificationIntent, ProjectRemoved, ProjectUpserted, SessionRemoved, SessionSummary,
};
use crate::interaction::{InteractionRequested, InteractionResolved};
use crate::item::{ItemCompleted, ItemDelta, ItemStarted, ItemUpdated};
use crate::open_union;
use crate::session::{
    QueueUpdated, SessionConfigured, SnapshotRecorded, TurnCompleted, TurnStarted, UsageUpdated,
    WorkspaceFailed, WorkspacePreparing, WorkspaceReady,
};
use crate::text::escaped_len;

open_union! {
    /// The `type` and `data` of an event. A snapshot or a history page is a
    /// list of these, applied the same way as live events (ADR-06).
    pub enum EventBody in tag "type", content "data" {
        ProjectUpserted(ProjectUpserted) => "project.upserted",
        ProjectRemoved(ProjectRemoved) => "project.removed",
        SessionUpserted(SessionSummary) => "session.upserted",
        SessionRemoved(SessionRemoved) => "session.removed",
        AgentsUpdated(AgentsUpdated) => "agents.updated",
        HostUpdated(HostUpdated) => "host.updated",
        ClientsUpdated(ClientsUpdated) => "clients.updated",
        ElevationRequested(ElevationRequested) => "elevation.requested",
        ElevationResolved(ElevationResolved) => "elevation.resolved",
        NotificationIntent(NotificationIntent) => "notification.intent",
        SessionConfigured(SessionConfigured) => "session.configured",
        SessionCapabilities(SessionCapabilities) => "session.capabilities",
        WorkspacePreparing(WorkspacePreparing) => "workspace.preparing",
        WorkspaceReady(WorkspaceReady) => "workspace.ready",
        WorkspaceFailed(WorkspaceFailed) => "workspace.failed",
        TurnStarted(TurnStarted) => "turn.started",
        TurnCompleted(TurnCompleted) => "turn.completed",
        ItemStarted(ItemStarted) => "item.started",
        /// Mergeable by `(item_id, field)`: see the [`Mergeable`] rule.
        #[schemars(extend("x-mergeable" = true))]
        ItemDelta(ItemDelta) => "item.delta",
        ItemUpdated(ItemUpdated) => "item.updated",
        ItemCompleted(ItemCompleted) => "item.completed",
        InteractionRequested(InteractionRequested) => "interaction.requested",
        InteractionResolved(InteractionResolved) => "interaction.resolved",
        QueueUpdated(QueueUpdated) => "queue.updated",
        SnapshotRecorded(SnapshotRecorded) => "snapshot.recorded",
        UsageUpdated(UsageUpdated) => "usage.updated",
    }
}

/// The two kinds of stream (design 5.3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamKind {
    Index,
    Session,
}

/// What a reader does with an event whose `type` it knows but whose data
/// does not parse (ADR-04 revision 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Strictness {
    /// Fail: CI checks golden files this way.
    Strict,
    /// Degrade the whole event to `Unknown` and report why: clients.
    Tolerant,
}

/// A decoded event or body.
#[derive(Debug)]
pub struct Decoded<T> {
    pub value: T,
    /// Set when [`Strictness::Tolerant`] degraded a known type; the caller logs it.
    pub degraded: Option<serde_json::Error>,
}

impl EventBody {
    /// The stream this type of event belongs to; `None` for an unknown type.
    pub fn stream_kind(&self) -> Option<StreamKind> {
        match self {
            Self::ProjectUpserted(_)
            | Self::ProjectRemoved(_)
            | Self::SessionUpserted(_)
            | Self::SessionRemoved(_)
            | Self::AgentsUpdated(_)
            | Self::HostUpdated(_)
            | Self::ClientsUpdated(_)
            | Self::ElevationRequested(_)
            | Self::ElevationResolved(_)
            | Self::NotificationIntent(_) => Some(StreamKind::Index),
            Self::SessionConfigured(_)
            | Self::SessionCapabilities(_)
            | Self::WorkspacePreparing(_)
            | Self::WorkspaceReady(_)
            | Self::WorkspaceFailed(_)
            | Self::TurnStarted(_)
            | Self::TurnCompleted(_)
            | Self::ItemStarted(_)
            | Self::ItemDelta(_)
            | Self::ItemUpdated(_)
            | Self::ItemCompleted(_)
            | Self::InteractionRequested(_)
            | Self::InteractionResolved(_)
            | Self::QueueUpdated(_)
            | Self::SnapshotRecorded(_)
            | Self::UsageUpdated(_) => Some(StreamKind::Session),
            Self::Unknown(_) => None,
        }
    }

    /// Decodes a body without its envelope, as snapshots and history pages carry it.
    pub fn decode(raw: &Value, strictness: Strictness) -> Result<Decoded<Self>, serde_json::Error> {
        decode(raw, strictness, |raw| {
            Ok(Self::Unknown(UnknownEvent::deserialize(raw)?))
        })
    }
}

impl Mergeable for EventBody {
    /// An `item.delta` followed by one of the same `(item_id, field)` merges
    /// while the merged text stays within `max_escaped` escaped bytes.
    fn merge(&self, next: &Self, max_escaped: usize) -> Option<Self> {
        let (Self::ItemDelta(a), Self::ItemDelta(b)) = (self, next) else {
            return None;
        };
        let fits = escaped_len(&a.text) + escaped_len(&b.text) <= max_escaped;
        (a.item_id == b.item_id && a.field == b.field && fits).then(|| {
            Self::ItemDelta(ItemDelta {
                item_id: a.item_id.clone(),
                field: a.field.clone(),
                text: format!("{}{}", a.text, b.text),
            })
        })
    }
}

impl Event<EventBody> {
    /// Decodes an event with its envelope. A malformed envelope fails in
    /// either mode: there is no seq to place the event at.
    pub fn decode(raw: &Value, strictness: Strictness) -> Result<Decoded<Self>, serde_json::Error> {
        decode(raw, strictness, |raw| {
            let e = Event::<UnknownEvent>::deserialize(raw)?;
            Ok(Event {
                seq: e.seq,
                from: e.from,
                v: e.v,
                ts: e.ts,
                cause: e.cause,
                fallback: e.fallback,
                body: EventBody::Unknown(e.body),
            })
        })
    }
}

/// `T` as is; in tolerant mode, `degrade(raw)` when `T` does not parse.
fn decode<T: DeserializeOwned>(
    raw: &Value,
    strictness: Strictness,
    degrade: impl FnOnce(&Value) -> Result<T, serde_json::Error>,
) -> Result<Decoded<T>, serde_json::Error> {
    let (value, degraded) = match (T::deserialize(raw), strictness) {
        (Ok(value), _) => (value, None),
        (Err(error), Strictness::Strict) => return Err(error),
        (Err(error), Strictness::Tolerant) => match degrade(raw) {
            Ok(value) => (value, Some(error)),
            Err(_) => return Err(error),
        },
    };
    Ok(Decoded { value, degraded })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// One event of every type; optional fields present unless a second
    /// sample of the type leaves them out.
    fn samples() -> Vec<Value> {
        let s1 = "s_0190a3b4c5d67e8f9a0b1c2d3e4f5a6b";
        [
            json!({"type": "project.upserted", "data": {"project_id": "p_1", "name": "cupper",
                "path": "/work/cupper", "is_git": true, "added_at": 1, "position": 0, "trust_state": "stale"}}),
            json!({"type": "project.removed", "data": {"project_id": "p_1"}}),
            json!({"type": "session.upserted", "data": {"session_id": s1, "project_id": "p_1",
                "agent": "claude", "title": "Fix the build", "title_source": "agent",
                "workspace_kind": "worktree", "branch": "cupper/fix", "status": "needs_action",
                "unread": true, "pending_interactions": 2, "pending_kind": "approval",
                "last_activity_at": 5, "error_code": "quota_exhausted", "read_only": false}}),
            json!({"type": "session.upserted", "data": {"session_id": s1, "project_id": "p_1",
                "agent": "codex", "title": "", "title_source": "none", "workspace_kind": "local",
                "status": "idle", "unread": false, "pending_interactions": 0, "last_activity_at": 5,
                "read_only": true}}),
            json!({"type": "session.removed", "data": {"session_id": s1, "reason": "archived"}}),
            json!({"type": "agents.updated", "data": {"agents": [
                {"agent": "claude", "state": "running",
                 "cli": {"path": "/opt/homebrew/bin/claude", "version": "2.1.296", "compat": "verified",
                         "install": "native"},
                 "adapter": {"version": "0.91.0", "runtime": "failed", "failure": "hash_mismatch"},
                 "auth": {"state": "logged_in", "kind": "subscription", "plan": "max"},
                 "rss_bytes": 734003200, "config_dir": "ok"},
                {"agent": "codex", "state": "unavailable", "adapter": {"version": "2.2.2", "runtime": "missing"},
                 "auth": {"state": "unchecked"}, "config_dir": "default"}],
                "git": {"version": "2.50.1", "lfs": "3.7.0"}}}),
            json!({"type": "agents.updated", "data": {"agents": [], "git": {}}}),
            json!({"type": "host.updated", "data": {"version": "0.1.0", "weak_auth": false,
                "pending_upgrade": true, "power_assertion": true, "last_exit_abnormal": true}}),
            json!({"type": "clients.updated", "data": {"clients": [
                {"conn_id": "c_1", "principal_kind": "desktop_ui", "role": "window", "client_name": "Cupper",
                 "client_version": "0.1.0", "since": 3, "auth": "verified"},
                {"conn_id": "c_2", "principal_kind": "cli", "client_name": "cupper-cli", "client_version": "0.1.0",
                 "since": 4, "auth": "peer", "elevated": {"scopes": ["write", "approve"], "expires_at": 9}}]}}),
            json!({"type": "elevation.requested", "data": {"elevation_id": "e_1", "client_name": "cupper-cli",
                "scopes": ["write"], "requested_at": 7}}),
            json!({"type": "elevation.resolved", "data": {"elevation_id": "e_1", "decision": "expired"}}),
            json!({"type": "notification.intent", "data": {"intent_id": "n_1", "session_id": s1,
                "reason": "needs_action", "request_id": "r_1"}}),
        ]
        .into_iter()
        .collect()
    }

    fn envelope(body: &Value) -> Value {
        let mut event = json!({"seq": 42, "v": 1, "ts": 1700000000000_u64});
        event
            .as_object_mut()
            .unwrap()
            .extend(body.as_object().unwrap().clone());
        event
    }

    /// Decodes `raw` as an event and as a bare body; both reproduce `raw`.
    fn decode_both(body: &Value, strictness: Strictness) -> (EventBody, bool) {
        let event =
            Event::decode(&envelope(body), strictness).unwrap_or_else(|e| panic!("{body}: {e}"));
        assert_eq!(serde_json::to_value(&event.value).unwrap(), envelope(body));
        let bare = EventBody::decode(body, strictness).unwrap();
        assert_eq!(bare.value, event.value.body);
        assert_eq!(bare.degraded.is_some(), event.degraded.is_some());
        (bare.value, bare.degraded.is_some())
    }

    /// A known type with malformed data: strict decoding fails with `why`;
    /// tolerant decoding degrades the whole event to `Unknown` and keeps its data.
    fn rejected_or_degraded(body: &Value, why: &str) {
        let err = Event::decode(&envelope(body), Strictness::Strict).unwrap_err();
        assert!(err.to_string().contains(why), "{body}: {err}");
        assert!(EventBody::decode(body, Strictness::Strict).is_err());

        let event = Event::decode(&envelope(body), Strictness::Tolerant).unwrap();
        assert!(event.degraded.unwrap().to_string().contains(why));
        let EventBody::Unknown(unknown) = &event.value.body else {
            panic!("{body}")
        };
        assert_eq!(unknown.kind, body["type"]);
        assert_eq!(event.value.seq, 42);
        assert_eq!(serde_json::to_value(&unknown.data).unwrap(), body["data"]);
        let bare = EventBody::decode(body, Strictness::Tolerant).unwrap();
        assert!(bare.degraded.is_some());
    }

    /// Every sample decodes strictly, reproduces itself and belongs to
    /// `stream`; together the samples cover `tags`, in order.
    fn covers(samples: Vec<Value>, stream: StreamKind, tags: &[&str]) {
        let mut seen = Vec::new();
        for body in samples {
            let (decoded, degraded) = decode_both(&body, Strictness::Strict);
            assert!(!degraded);
            assert_eq!(decoded.tag(), body["type"]);
            assert_eq!(decoded.stream_kind(), Some(stream), "{body}");
            seen.push(decoded.tag().to_owned());
        }
        seen.dedup();
        assert_eq!(seen, tags);
    }

    #[test]
    fn unit_proto_events_index_every_type_roundtrips() {
        covers(samples(), StreamKind::Index, &EventBody::TAGS[..10]);
    }

    #[test]
    fn unit_proto_events_index_unknown_types_and_values() {
        let future =
            json!({"type": "project.pinned", "data": {"project_id": "p_1", "pinned": true}});
        for strictness in [Strictness::Strict, Strictness::Tolerant] {
            let (decoded, degraded) = decode_both(&future, strictness);
            assert!(!degraded);
            assert!(matches!(&decoded, EventBody::Unknown(e) if e.kind == "project.pinned"));
            assert_eq!(decoded.stream_kind(), None);
        }

        // Unknown enum values and extra fields in a known type are not malformed.
        let newer = json!({"type": "session.upserted", "data": {"session_id": "s_1", "project_id": "p_1",
            "agent": "gemini", "title": "t", "title_source": "summary", "workspace_kind": "container",
            "status": "paused", "unread": false, "pending_interactions": 1, "pending_kind": "review",
            "last_activity_at": 1, "error_code": "disk_full", "read_only": false}});
        let (decoded, degraded) = decode_both(&newer, Strictness::Strict);
        assert!(!degraded);
        let EventBody::SessionUpserted(summary) = decoded else {
            panic!("{decoded:?}")
        };
        assert_eq!(summary.status.as_str(), "paused");
        let mut extra = newer.clone();
        extra["data"]["pinned"] = json!(true);
        let decoded = EventBody::decode(&extra, Strictness::Strict).unwrap();
        assert_eq!(serde_json::to_value(&decoded.value).unwrap(), newer);
    }

    #[test]
    fn unit_proto_events_index_malformed_known_types() {
        let sample = |i: usize| samples().swap_remove(i);
        let mut cases = Vec::new();
        let mut missing = sample(0);
        missing["data"]
            .as_object_mut()
            .unwrap()
            .remove("trust_state");
        cases.push((missing, "missing field `trust_state`"));
        let mut negative = sample(0);
        negative["data"]["position"] = json!(-1);
        cases.push((negative, "invalid value"));
        let mut prefix = sample(9);
        prefix["data"]["elevation_id"] = json!("r_1");
        cases.push((prefix, "malformed ElevationId"));
        let mut nested = sample(5);
        nested["data"]["agents"][1]
            .as_object_mut()
            .unwrap()
            .remove("adapter");
        cases.push((nested, "missing field `adapter`"));
        cases.push((json!({"type": "project.removed"}), "invalid type: null"));
        cases.push((
            json!({"type": "host.updated", "data": "up"}),
            "invalid type: string",
        ));

        for (body, why) in cases {
            rejected_or_degraded(&body, why);
        }

        // Without a valid envelope or tag there is nothing to degrade to.
        let mut no_seq = envelope(&sample(1));
        no_seq.as_object_mut().unwrap().remove("seq");
        assert!(Event::decode(&no_seq, Strictness::Tolerant).is_err());
        let untagged = json!({"data": {"project_id": "p_1"}});
        assert!(EventBody::decode(&untagged, Strictness::Tolerant).is_err());
    }

    /// One event of every session-stream type, in `TAGS` order, covering every
    /// item kind, request type and outcome, with and without optional fields.
    fn session_samples() -> Vec<Value> {
        let blob = json!({"type": "blob", "preview": "fn main",
            "blob": {"hash": "ab".repeat(32), "size": 70000, "media_type": "text/plain"}});
        let started = |kind: Value| {
            let mut data = json!({"item_id": "i_1", "turn_id": "t_1", "started_at": 5});
            data.as_object_mut()
                .unwrap()
                .extend(kind.as_object().unwrap().clone());
            json!({"type": "item.started", "data": data})
        };
        let requested = |request: Value| {
            json!({"type": "interaction.requested", "data": {
            "request_id": "r_1", "created_at": 6, "digest": format!("d_{}", "0".repeat(64)),
            "actionable_by": ["desktop_ui", "cli"], "request": request}})
        };
        let resolved = |status: &str, more: Value| {
            let mut data = json!({"request_id": "r_1", "status": status});
            data.as_object_mut()
                .unwrap()
                .extend(more.as_object().unwrap().clone());
            json!({"type": "interaction.resolved", "data": data})
        };
        let allow = json!({"option_id": "allow-once", "name": "Allow", "kind": "allow_once",
            "effect": {"scope": "once", "persistent": false, "ends_turn": false}, "requires": "approve"});
        let always = json!({"option_id": "allow-with-updates", "name": "Always allow `npm test`",
            "kind": "allow_always", "requires": "privileged", "effect": {"scope": "persistent",
            "persistent": true, "rule": {"text": "Bash(npm test:*)", "location": "localSettings",
            "prefix": "npm test", "prefix_risk": "normal"}}});
        let auto = json!({"option_id": "exit-plan-auto", "name": "Auto", "kind": "allow_always",
            "requires": "privileged", "effect": {"scope": "mode_switch", "persistent": false,
            "ends_turn": false, "mode_after": "auto", "tier_after": "elevated"}});
        let odd = json!({"option_id": "x", "name": "Do the thing", "kind": "allow_sometimes",
            "requires": "privileged", "effect": {"scope": "unknown", "persistent": true,
            "rule": {"prefix_risk": "unknown"}}});
        vec![
            json!({"type": "session.configured", "data": {"agent": "claude", "workspace": {"kind": "worktree",
                "path": "/w/fix", "branch": "cupper/fix", "base_ref": "main"},
                "config": {"model": "opus", "permission_mode": "bypassPermissions"}, "read_only": false,
                "project_config": "ignored", "confirmation_required": {"reason": "full_access_unconfirmed",
                "config_digest": format!("d_{}", "1".repeat(64))}}}),
            json!({"type": "session.configured", "data": {"agent": "codex", "config": {},
                "workspace": {"kind": "local", "path": "/w"}, "read_only": true, "project_config": "loaded"}}),
            json!({"type": "session.capabilities", "data": {"agent": "codex",
                "versions": {"adapter": "2.2.2", "cli": "0.160.0", "cli_compat": "verified"},
                "prompt": {"image": true, "file_ref": true, "max_image_bytes": 10485760},
                "turn": {"queue": true, "steer": {"supported": true, "side_effects": false},
                         "interrupt": true, "interrupt_stops_commands": false},
                "session": {"resume": true, "load_history": false, "fork": false}, "config": [], "commands": [],
                "interactions": {"reject_ends_turn": true, "question_forms": true, "plan_confirm": true},
                "extraction": {"exit_code": true, "output_streaming": true, "diff": "per_hunk",
                               "thinking": true, "plan": "entries", "run_state": "agent_reported"}}}),
            json!({"type": "workspace.preparing", "data": {"phase": "checkout", "progress": 40}}),
            json!({"type": "workspace.preparing", "data": {"phase": "worktree_add"}}),
            json!({"type": "workspace.ready", "data": {"path": "/w/fix", "branch": "cupper/fix"}}),
            json!({"type": "workspace.failed", "data": {"code": "git_error", "reason": "lock_held"}}),
            json!({"type": "turn.started", "data": {"turn_id": "t_1", "turn_no": 3, "origin": "steer_new_turn",
                "started_at": 5}}),
            json!({"type": "turn.completed", "data": {"turn_id": "t_1", "status": "failed",
                "stop_reason": "host_timeout", "ended_at": 9, "duration_ms": 4, "error_code": "adapter_exited",
                "usage": {"input": 1200, "output": 300, "cached": 1000, "cost": {"amount": 0.5, "currency": "USD"}},
                "final_item_ids": ["i_2", "i_3"]}}),
            json!({"type": "turn.completed", "data": {"turn_id": "t_1", "status": "completed", "ended_at": 9,
                "duration_ms": 4, "usage": {"input": 1, "output": 2}, "final_item_ids": []}}),
            started(
                json!({"kind": "user_message", "content": [{"type": "text", "text": "No, use pnpm"}],
                "steered": true, "queue_id": "q_1", "reply_to": "r_0"}),
            ),
            started(json!({"kind": "agent_message"})),
            started(json!({"kind": "thought", "group_id": "g_1"})),
            started(
                json!({"kind": "tool_call", "group_id": "g_1", "tool_kind": "execute", "title": "Run tests",
                "paths": [], "command": "cargo test", "cwd": "/w", "exit_code": 101, "signal": "SIGTERM",
                "output": blob, "output_state": "truncated", "output_tail": "FAILED", "output_bytes": 70000,
                "diffs": [{"path": "a.rs", "change": "update", "hunks": blob, "stat": {"added": 2, "removed": 1}}],
                "parent_item_id": "i_0", "extraction": "partial"}),
            ),
            started(
                json!({"kind": "tool_call", "tool_kind": "fetch", "title": "Fetch", "paths": [],
                "url": "https://example.com", "query": "q", "output_state": "none", "extraction": "full"}),
            ),
            started(
                json!({"kind": "late_tool_output", "ref": {"item_id": "i_0", "turn_id": "t_0",
                "title": "Run server", "command": "npm start"}, "output_state": "streaming"}),
            ),
            started(
                json!({"kind": "plan", "source": "entries", "entries": [{"content": "Write tests",
                "status": "completed", "priority": "high"}, {"content": "Fix", "status": "pending"}]}),
            ),
            started(
                json!({"kind": "plan", "source": "markdown", "markdown": {"type": "text", "text": "1. a"}}),
            ),
            started(
                json!({"kind": "notice", "code": "repo_config_changed", "params": {"paths": [".git/config"]}}),
            ),
            started(
                json!({"kind": "error", "code": "not_logged_in", "agent_code": "-32000", "retryable": false,
                "guidance": "run_login_in_terminal", "raw": "Authentication required"}),
            ),
            json!({"type": "item.delta", "data": {"item_id": "i_1", "field": "output", "text": "ok\n"}}),
            json!({"type": "item.updated", "data": {"item_id": "i_1", "output_state": "truncated",
                "output_tail": "…", "output_bytes": 20000, "params": {"attempt": 2, "max": 5}}}),
            json!({"type": "item.completed", "data": {"item_id": "i_1", "status": "abandoned", "ended_at": 9,
                "duration_ms": 4, "may_still_run": true, "exit_code": 0, "output": blob, "output_state": "complete"}}),
            json!({"type": "item.completed", "data": {"item_id": "i_1", "status": "completed", "ended_at": 9,
                "duration_ms": 0}}),
            requested(
                json!({"type": "permission", "title": "Run npm test", "options": [allow, always, odd],
                "tool": {"item_id": "i_1", "tool_kind": "execute", "command": blob, "paths": ["package.json"]},
                "detail": {"type": "text", "text": "in /w"}, "content_truncated": true}),
            ),
            requested(
                json!({"type": "question", "prompt": "Which?", "unsupported_schema": false, "fields": [
                {"field_id": "question_1", "label": "Database", "type": "single_choice", "allow_custom": true,
                 "required": true, "choices": [{"value": "pg", "label": "Postgres", "description": "SQL"}]},
                {"field_id": "n", "label": "Count", "type": "number", "allow_custom": false, "required": false}]}),
            ),
            requested(
                json!({"type": "plan_confirm", "plan": {"type": "text", "text": "1. a"}, "options": [auto]}),
            ),
            requested(
                json!({"type": "auth_url", "url": "https://example.com/login", "message": "Sign in"}),
            ),
            resolved(
                "resolved",
                json!({"outcome": {"type": "selected", "option_id": "allow-once"},
                "resolved_by": {"principal_id": "pr_desktop", "client_kind": "desktop", "client_name": "Cupper"}}),
            ),
            resolved(
                "resolved",
                json!({"outcome": {"type": "answer", "values": {"question_1": "pg", "n": 3}}}),
            ),
            resolved("resolved", json!({"outcome": {"type": "dismissed"}})),
            resolved("resolved", json!({"outcome": {"type": "opened"}})),
            resolved("expired", json!({"reason": "adapter_exited"})),
            json!({"type": "queue.updated", "data": {"version": 7, "paused": {"reason": "config_unconfirmed"},
                "entries": [{"queue_id": "q_1", "preview": "And docs", "created_at": 8, "created_by": "pr_cli",
                "content": [{"type": "text", "text": "And docs"}, {"type": "file_ref", "path": "README.md"}],
                "reply_to": "r_1"}]}}),
            json!({"type": "queue.updated", "data": {"version": 8, "entries": []}}),
            json!({"type": "snapshot.recorded", "data": {"turn_id": "t_1", "phase": "end", "status": "partial",
                "excluded": [{"path": "data.bin", "reason": "too_large"}], "stat": {"files": 3, "added": 40, "removed": 2}}}),
            json!({"type": "snapshot.recorded", "data": {"turn_id": "t_1", "phase": "start", "status": "skipped",
                "reason": "not_a_repo", "excluded": []}}),
            json!({"type": "usage.updated", "data": {"context_used": 50000, "context_size": 200000,
                "cost": {"amount": 1.25, "currency": "USD"}}}),
            json!({"type": "usage.updated", "data": {"context_used": 0, "context_size": 258400}}),
        ]
    }

    #[test]
    fn unit_proto_events_session_every_type_roundtrips() {
        covers(
            session_samples(),
            StreamKind::Session,
            &EventBody::TAGS[10..],
        );
    }

    #[test]
    fn unit_proto_events_session_unknown_members_and_values() {
        let sample = |tag: &str| {
            session_samples()
                .into_iter()
                .find(|s| s["type"] == tag)
                .unwrap()
        };
        let mut item = sample("item.started");
        item["data"]["kind"] = json!("canvas");
        let mut request = sample("interaction.requested");
        request["data"]["request"] = json!({"type": "oauth_device", "code": "ABCD"});
        let mut outcome = sample("interaction.resolved");
        outcome["data"]["outcome"] = json!({"type": "later", "at": 5});
        let newer = [
            json!({"type": "item.delta", "data": {"item_id": "i_1", "field": "stderr", "text": "x"}}),
            json!({"type": "item.started", "data": {"item_id": "i_1", "started_at": 1, "kind": "tool_call",
                "tool_kind": "browse", "title": "t", "paths": [], "output_state": "paused", "extraction": "guess"}}),
            json!({"type": "turn.completed", "data": {"turn_id": "t_1", "status": "paused", "stop_reason": "nap",
                "ended_at": 9, "duration_ms": 4, "error_code": "disk_full", "final_item_ids": []}}),
        ];
        for body in [item, request, outcome].into_iter().chain(newer) {
            let decoded = EventBody::decode(&body, Strictness::Strict)
                .unwrap_or_else(|e| panic!("{body}: {e}"));
            assert!(decoded.degraded.is_none());
            assert_eq!(decoded.value.stream_kind(), Some(StreamKind::Session));
            assert_eq!(serde_json::to_value(&decoded.value).unwrap(), body);
        }
    }

    #[test]
    fn unit_proto_events_session_malformed_known_types() {
        let sample = |i: usize| session_samples().swap_remove(i);
        let mut cases = Vec::new();
        let mut final_ids = sample(8);
        final_ids["data"]
            .as_object_mut()
            .unwrap()
            .remove("final_item_ids");
        cases.push((final_ids, "missing field `final_item_ids`"));
        let mut turn_no = sample(7);
        turn_no["data"]["turn_no"] = json!(-1);
        cases.push((turn_no, "invalid value"));
        let mut tool = sample(13);
        tool["data"].as_object_mut().unwrap().remove("output_state");
        cases.push((tool, "`tool_call`: missing field `output_state`"));
        let mut delta = sample(20);
        delta["data"]["item_id"] = json!("t_1");
        cases.push((delta, "malformed ItemId"));
        let mut effect = sample(24);
        effect["data"]["request"]["options"][0]["effect"]
            .as_object_mut()
            .unwrap()
            .remove("persistent");
        cases.push((effect, "`permission`: missing field `persistent`"));
        let mut selected = sample(28);
        selected["data"]["outcome"]
            .as_object_mut()
            .unwrap()
            .remove("option_id");
        cases.push((selected, "`selected`: missing field `option_id`"));
        let mut block = sample(33);
        block["data"]["entries"][0]["content"][1] = json!({"type": "file_ref"});
        cases.push((block, "`file_ref`: missing field `path`"));
        for (body, why) in cases {
            rejected_or_degraded(&body, why);
        }
    }

    #[test]
    fn unit_proto_events_session_item_delta_merges_by_item_and_field() {
        let delta = |item: &str, field: &str, text: &str| -> Value {
            json!({"type": "item.delta", "data": {"item_id": item, "field": field, "text": text}})
        };
        let other =
            json!({"type": "usage.updated", "data": {"context_used": 1, "context_size": 2}});
        let bodies = [
            delta("i_1", "text", "a"),
            delta("i_1", "text", "b"),
            delta("i_1", "output", "c"),
            delta("i_1", "output", "d"),
            delta("i_2", "output", "e"),
            other.clone(),
            delta("i_2", "output", "f"),
        ];
        let events: Vec<Event<EventBody>> = (1..)
            .zip(bodies)
            .map(|(seq, body)| {
                let mut raw = envelope(&body);
                raw["seq"] = json!(seq);
                serde_json::from_value(raw).unwrap()
            })
            .collect();
        let merged = crate::coalesce::coalesce(events, crate::text::MAX_MERGED_ESCAPED);
        let got: Vec<(u64, u64, Value)> = merged
            .iter()
            .map(|e| (e.first_seq(), e.seq, serde_json::to_value(&e.body).unwrap()))
            .collect();
        let expected = [
            (1, 2, delta("i_1", "text", "ab")),
            (3, 4, delta("i_1", "output", "cd")),
            (5, 5, delta("i_2", "output", "e")),
            (6, 6, other),
            (7, 7, delta("i_2", "output", "f")),
        ];
        assert_eq!(got, expected);

        // The cap counts escaped bytes: `"` takes two.
        let body = |v: Value| EventBody::decode(&v, Strictness::Strict).unwrap().value;
        let (ab, quote, cd) = (
            body(delta("i_1", "text", "ab")),
            body(delta("i_1", "text", "\"")),
            body(delta("i_1", "text", "cd")),
        );
        assert_eq!(
            ab.merge(&quote, 4),
            Some(body(delta("i_1", "text", "ab\"")))
        );
        assert_eq!(ab.merge(&quote, 3), None);
        assert_eq!(
            ab.merge(&cd, 4).and_then(|abcd| abcd.merge(&quote, 5)),
            None
        );
        let unknown = body(json!({"type": "item.delta2", "data": {}}));
        assert_eq!(unknown.merge(&unknown, 100), None);
    }

    #[test]
    fn unit_proto_events_session_only_item_delta_is_mergeable() {
        let schema = serde_json::to_value(schemars::schema_for!(EventBody)).unwrap();
        let members = schema["anyOf"].as_array().unwrap();
        assert_eq!(
            members.len(),
            EventBody::TAGS.len() + 1,
            "the known types plus the fallback"
        );
        let mergeable: Vec<&Value> = members
            .iter()
            .filter(|m| m.get(crate::annotations::X_MERGEABLE) == Some(&json!(true)))
            .map(|m| &m["properties"]["type"]["const"])
            .collect();
        assert_eq!(mergeable, ["item.delta"]);
    }
}
