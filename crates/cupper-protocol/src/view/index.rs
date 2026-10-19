//! The index stream's view: projects, session summaries, agents, the host,
//! connected clients and pending elevation requests. It has no window: a
//! snapshot carries all of it.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{View, ViewError};
use crate::event::EventBody;
use crate::ids::{ElevationId, ProjectId, SessionId};
use crate::index::{
    AgentsUpdated, ClientsUpdated, ElevationRequested, HostUpdated, ProjectUpserted, SessionSummary,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IndexView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<HostUpdated>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<AgentsUpdated>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clients: Option<ClientsUpdated>,
    pub projects: BTreeMap<ProjectId, ProjectUpserted>,
    /// Unarchived sessions.
    pub sessions: BTreeMap<SessionId, SessionSummary>,
    /// Requests not yet granted, denied or expired.
    pub elevations: BTreeMap<ElevationId, ElevationRequested>,
}

impl View for IndexView {
    /// A later `host`, `agents` or `clients` update replaces the earlier one;
    /// removing what is not there is an error. `notification.intent` is a
    /// one-off signal and never part of the view.
    fn apply(&mut self, body: &EventBody) -> Result<(), ViewError> {
        use EventBody as B;
        fn removed<K: Ord + ToString, V>(
            map: &mut BTreeMap<K, V>,
            key: &K,
        ) -> Result<(), ViewError> {
            map.remove(key)
                .map(drop)
                .ok_or_else(|| ViewError::Missing(key.to_string()))
        }
        match body {
            B::HostUpdated(h) => self.host = Some(h.clone()),
            B::AgentsUpdated(a) => self.agents = Some(a.clone()),
            B::ClientsUpdated(c) => self.clients = Some(c.clone()),
            B::ProjectUpserted(p) => {
                self.projects.insert(p.project_id.clone(), p.clone());
            }
            B::ProjectRemoved(p) => removed(&mut self.projects, &p.project_id)?,
            B::SessionUpserted(s) => {
                self.sessions.insert(s.session_id.clone(), s.clone());
            }
            B::SessionRemoved(s) => removed(&mut self.sessions, &s.session_id)?,
            B::ElevationRequested(e) => {
                self.elevations.insert(e.elevation_id.clone(), e.clone());
            }
            B::ElevationResolved(e) => removed(&mut self.elevations, &e.elevation_id)?,
            B::NotificationIntent(_) | B::Unknown(_) => {}
            B::SessionConfigured(_)
            | B::SessionCapabilities(_)
            | B::WorkspacePreparing(_)
            | B::WorkspaceReady(_)
            | B::WorkspaceFailed(_)
            | B::TurnStarted(_)
            | B::TurnCompleted(_)
            | B::ItemStarted(_)
            | B::ItemDelta(_)
            | B::ItemUpdated(_)
            | B::ItemCompleted(_)
            | B::InteractionRequested(_)
            | B::InteractionResolved(_)
            | B::QueueUpdated(_)
            | B::SnapshotRecorded(_)
            | B::UsageUpdated(_) => return Err(ViewError::OtherStream(body.tag().to_owned())),
        }
        Ok(())
    }

    fn snapshot_ops(&self) -> Vec<EventBody> {
        let singles = [
            self.host.clone().map(EventBody::HostUpdated),
            self.agents.clone().map(EventBody::AgentsUpdated),
            self.clients.clone().map(EventBody::ClientsUpdated),
        ];
        let projects = self
            .projects
            .values()
            .cloned()
            .map(EventBody::ProjectUpserted);
        let sessions = self
            .sessions
            .values()
            .cloned()
            .map(EventBody::SessionUpserted);
        let elevations = self.elevations.values().cloned();
        singles
            .into_iter()
            .flatten()
            .chain(projects)
            .chain(sessions)
            .chain(elevations.map(EventBody::ElevationRequested))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::{Value, json};

    use super::*;
    use crate::event::Strictness;
    use crate::view::paginate;

    fn body(raw: Value) -> EventBody {
        EventBody::decode(&raw, Strictness::Strict)
            .unwrap_or_else(|e| panic!("{raw}: {e}"))
            .value
    }

    /// An index event of type `kind % 10` about the entity numbered `n`.
    fn event(kind: u8, n: u8) -> EventBody {
        let (p, s, e) = (format!("p_{n}"), format!("s_{n}"), format!("e_{n}"));
        body(match kind % 10 {
            0 => {
                json!({"type": "project.upserted", "data": {"project_id": p, "name": format!("Project \"{n}\""),
                "path": format!("/w/{n}"), "is_git": n.is_multiple_of(2), "added_at": n, "position": n, "trust_state": "trusted"}})
            }
            1 => json!({"type": "project.removed", "data": {"project_id": p}}),
            2 => {
                json!({"type": "session.upserted", "data": {"session_id": s, "project_id": p, "agent": "codex",
                "title": format!("Session {n}"), "title_source": "prompt", "workspace_kind": "local", "status": "idle",
                "unread": n.is_multiple_of(3), "pending_interactions": n, "last_activity_at": n, "read_only": false}})
            }
            3 => {
                json!({"type": "session.removed", "data": {"session_id": s, "reason": "archived"}})
            }
            4 => {
                json!({"type": "host.updated", "data": {"version": format!("0.{n}.0"), "weak_auth": false,
                "pending_upgrade": n % 2 == 1, "power_assertion": true}})
            }
            5 => {
                json!({"type": "agents.updated", "data": {"agents": [{"agent": "claude", "state": "idle",
                "adapter": {"version": "0.91.0", "runtime": "ready"}, "auth": {"state": "unchecked"},
                "config_dir": "default", "rss_bytes": n}], "git": {"version": "2.50.1"}}})
            }
            6 => {
                json!({"type": "clients.updated", "data": {"clients": [{"conn_id": format!("c_{n}"),
                "principal_kind": "cli", "client_name": "cupper-cli", "client_version": "0.1.0", "since": n,
                "auth": "peer"}]}})
            }
            7 => {
                json!({"type": "elevation.requested", "data": {"elevation_id": e, "client_name": "cupper-cli",
                "scopes": ["write"], "requested_at": n}})
            }
            8 => {
                json!({"type": "elevation.resolved", "data": {"elevation_id": e, "decision": "denied"}})
            }
            _ => {
                json!({"type": "notification.intent", "data": {"intent_id": format!("n_{n}"), "session_id": s,
                "reason": "turn_completed"}})
            }
        })
    }

    #[test]
    fn unit_view_window_index_upserts_removes_and_ignores() {
        let mut view = IndexView::default();
        for (kind, n) in [
            (0, 1),
            (0, 2),
            (2, 1),
            (7, 1),
            (7, 2),
            (4, 1),
            (4, 2),
            (9, 1),
        ] {
            view.apply(&event(kind, n)).unwrap();
        }
        view.apply(&event(0, 1)).unwrap();
        for (kind, n) in [(1, 2), (8, 1)] {
            view.apply(&event(kind, n)).unwrap();
        }
        let ids = |v: &Value, key: &str| -> Vec<String> {
            v[key].as_object().unwrap().keys().cloned().collect()
        };
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(ids(&json, "projects"), ["p_1"]);
        assert_eq!(ids(&json, "sessions"), ["s_1"]);
        assert_eq!(ids(&json, "elevations"), ["e_2"]);
        assert_eq!(json["host"]["version"], "0.2.0", "the later update wins");
        assert!(json.get("agents").is_none(), "absent until reported");

        // Removing what is gone, or a session-stream event, is an error;
        // unknown types change nothing.
        let before = view.clone();
        assert_eq!(
            view.apply(&event(3, 7)),
            Err(ViewError::Missing("s_7".into()))
        );
        let turn = body(
            json!({"type": "turn.started", "data": {"turn_id": "t_1", "turn_no": 1,
            "origin": "user", "started_at": 1}}),
        );
        assert_eq!(
            view.apply(&turn),
            Err(ViewError::OtherStream("turn.started".into()))
        );
        view.apply(&body(json!({"type": "project.pinned", "data": {}})))
            .unwrap();
        assert_eq!(view, before);
    }

    proptest! {
        /// The index has no window, so its snapshot rebuilds all of it, in
        /// pages of any size.
        #[test]
        fn prop_view_coalesce_index_snapshot_pages_rebuild_the_view(
            steps in prop::collection::vec((0u8..10, 0u8..6), 0..60),
            page_bytes in 1usize..2_000,
        ) {
            let mut view = IndexView::default();
            for (kind, n) in steps {
                // A removal needs something to remove.
                if let Err(ViewError::Missing(_)) = view.clone().apply(&event(kind, n)) {
                    continue;
                }
                view.apply(&event(kind, n)).unwrap();
            }
            let ops = view.snapshot_ops();
            let mut rebuilt = IndexView::default();
            for page in paginate(&ops, page_bytes) {
                let json = serde_json::to_vec(&ops[page.clone()]).unwrap();
                prop_assert!(json.len() <= page_bytes || page.len() == 1);
                for op in &ops[page] {
                    rebuilt.apply(op).unwrap();
                }
            }
            prop_assert_eq!(&rebuilt, &view);
            let json = serde_json::to_value(&view).unwrap();
            prop_assert_eq!(serde_json::from_value::<IndexView>(json).unwrap(), view);
        }
    }
}
