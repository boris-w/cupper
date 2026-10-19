//! The event catalog (design 5.3.5): [`EventBody`] holds the `type` and
//! `data` of every event of both streams, and a reader decodes it either
//! strictly or tolerantly (design 5.3.11).

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::envelope::{Event, UnknownEvent};
use crate::index::{
    AgentsUpdated, ClientsUpdated, ElevationRequested, ElevationResolved, HostUpdated,
    NotificationIntent, ProjectRemoved, ProjectUpserted, SessionRemoved, SessionSummary,
};
use crate::open_union;

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

    #[test]
    fn unit_proto_events_index_every_type_roundtrips() {
        let mut seen = Vec::new();
        for body in samples() {
            let (decoded, degraded) = decode_both(&body, Strictness::Strict);
            assert!(!degraded);
            assert_eq!(decoded.tag(), body["type"]);
            assert_eq!(decoded.stream_kind(), Some(StreamKind::Index), "{body}");
            seen.push(decoded.tag().to_owned());
        }
        seen.dedup();
        assert_eq!(seen, EventBody::TAGS);
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
            let err = Event::decode(&envelope(&body), Strictness::Strict).unwrap_err();
            assert!(err.to_string().contains(why), "{body}: {err}");
            assert!(EventBody::decode(&body, Strictness::Strict).is_err());

            let event = Event::decode(&envelope(&body), Strictness::Tolerant).unwrap();
            assert!(event.degraded.unwrap().to_string().contains(why));
            let EventBody::Unknown(unknown) = &event.value.body else {
                panic!("{body}")
            };
            assert_eq!(unknown.kind, body["type"]);
            assert_eq!(event.value.seq, 42);
            assert_eq!(
                serde_json::to_value(&unknown.data).unwrap(),
                body["data"].clone()
            );
            assert!(
                EventBody::decode(&body, Strictness::Tolerant)
                    .unwrap()
                    .degraded
                    .is_some()
            );
        }

        // Without a valid envelope or tag there is nothing to degrade to.
        let mut no_seq = envelope(&sample(1));
        no_seq.as_object_mut().unwrap().remove("seq");
        assert!(Event::decode(&no_seq, Strictness::Tolerant).is_err());
        let untagged = json!({"data": {"project_id": "p_1"}});
        assert!(EventBody::decode(&untagged, Strictness::Tolerant).is_err());
    }
}
