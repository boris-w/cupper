//! The session stream's view (design 5.3.3, 5.2.8). Turns are ordered by
//! `turn_no`, so a history page inserts older turns in front; items and
//! interaction requests keep their arrival order inside their turn.
//!
//! An entry without `turn_id` (a notice, a request outside any turn) joins
//! the running turn, or, while no turn runs, the tail after the last turn,
//! which the next new turn takes in. Either way it is shown with that turn's
//! `turn_id` from then on, so the turn's snapshot carries it and a history
//! page puts it back in the same place.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{OpLimits, View, ViewError, Window};
use crate::annotations::{DurationMs, TimestampMs};
use crate::capabilities::SessionCapabilities;
use crate::event::EventBody;
use crate::ids::{ItemId, RequestId, TurnId};
use crate::interaction::{InteractionRequested, InteractionResolved, InteractionStatus};
use crate::item::{DeltaField, Item, ItemPatch, ItemStarted, ItemStatus};
use crate::session::{
    QueueUpdated, SessionConfigured, SnapshotRecorded, TurnCompleted, TurnStarted, UsageUpdated,
    WorkspaceFailed, WorkspacePreparing, WorkspaceReady,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured: Option<SessionConfigured>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<SessionCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<QueueUpdated>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageUpdated>,
    /// Ascending `turn_no`.
    pub turns: Vec<TurnView>,
    /// Entries after the last turn while no turn runs.
    pub tail: Vec<Entry>,
    pub items: BTreeMap<ItemId, ItemView>,
    pub requests: BTreeMap<RequestId, RequestView>,
    /// Pending requests whose turn left the window; the turn's history page
    /// puts them back in place.
    pub detached: Vec<RequestId>,
    /// `late_tool_output` items by the tool call they report on (`ref.item_id`),
    /// so the original row can show the final exit code (design 5.2.6).
    pub late_by_ref: BTreeMap<ItemId, ItemId>,
}

/// The latest `workspace.*` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum WorkspaceView {
    Preparing(WorkspacePreparing),
    Ready(WorkspaceReady),
    Failed(WorkspaceFailed),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TurnView {
    pub started: TurnStarted,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed: Option<TurnCompleted>,
    /// `snapshot.recorded`, one per phase.
    pub snapshots: Vec<SnapshotRecorded>,
    /// Items and requests in arrival order.
    pub entries: Vec<Entry>,
}

/// A row of the message stream: an item or a request, by ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
    Item(ItemId),
    Request(RequestId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemView {
    /// As started, with every `item.updated` and `item.completed` patch applied.
    pub item: ItemStarted,
    /// `item.delta` with `field: text`, joined.
    pub text: String,
    /// `item.delta` with `field: output`, joined. A complete output arrives
    /// in the tool's `output` field; `output_state` tells which to show.
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<ItemEnd>,
}

/// `item.completed` without its patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ItemEnd {
    pub status: ItemStatus,
    pub ended_at: TimestampMs,
    /// The tool call's duration.
    pub duration_ms: DurationMs,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub may_still_run: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RequestView {
    pub requested: InteractionRequested,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<InteractionResolved>,
}

impl RequestView {
    pub fn status(&self) -> &InteractionStatus {
        self.resolved
            .as_ref()
            .map_or(&InteractionStatus::Pending, |r| &r.status)
    }

    pub fn is_pending(&self) -> bool {
        *self.status() == InteractionStatus::Pending
    }
}

impl View for SessionView {
    /// Errors on an ID the view does not have or already has. A patch sets
    /// the fields the item's kind has and ignores the others; an unknown
    /// kind takes all of them.
    fn apply(&mut self, body: &EventBody) -> Result<(), ViewError> {
        use EventBody as B;
        match body {
            B::SessionConfigured(c) => self.configured = Some(c.clone()),
            B::SessionCapabilities(c) => self.capabilities = Some(c.clone()),
            B::WorkspacePreparing(w) => self.workspace = Some(WorkspaceView::Preparing(w.clone())),
            B::WorkspaceReady(w) => self.workspace = Some(WorkspaceView::Ready(w.clone())),
            B::WorkspaceFailed(w) => self.workspace = Some(WorkspaceView::Failed(w.clone())),
            B::QueueUpdated(q) => self.queue = Some(q.clone()),
            B::UsageUpdated(u) => self.usage = Some(u.clone()),
            B::TurnStarted(t) => self.start_turn(t)?,
            B::TurnCompleted(t) => self.turn_mut(&t.turn_id)?.completed = Some(t.clone()),
            B::SnapshotRecorded(s) => {
                let snapshots = &mut self.turn_mut(&s.turn_id)?.snapshots;
                snapshots.retain(|old| old.phase != s.phase);
                snapshots.push(s.clone());
            }
            B::ItemStarted(i) => self.start_item(i)?,
            B::ItemDelta(d) => {
                let item = self.item_mut(&d.item_id)?;
                match d.field {
                    DeltaField::Text => item.text.push_str(&d.text),
                    DeltaField::Output => item.output.push_str(&d.text),
                    DeltaField::Unknown(_) => {}
                }
            }
            B::ItemUpdated(u) => patch(&mut self.item_mut(&u.item_id)?.item.item, &u.patch),
            B::ItemCompleted(c) => {
                let item = self.item_mut(&c.item_id)?;
                patch(&mut item.item.item, &c.patch);
                item.end = Some(ItemEnd {
                    status: c.status.clone(),
                    ended_at: c.ended_at,
                    duration_ms: c.duration_ms,
                    may_still_run: c.may_still_run,
                });
            }
            B::InteractionRequested(r) => self.request(r)?,
            B::InteractionResolved(r) => {
                let request = self.requests.get_mut(&r.request_id);
                request.ok_or_else(|| missing(&r.request_id))?.resolved = Some(r.clone());
            }
            B::Unknown(_) => {}
            B::ProjectUpserted(_)
            | B::ProjectRemoved(_)
            | B::SessionUpserted(_)
            | B::SessionRemoved(_)
            | B::AgentsUpdated(_)
            | B::HostUpdated(_)
            | B::ClientsUpdated(_)
            | B::ElevationRequested(_)
            | B::ElevationResolved(_)
            | B::NotificationIntent(_) => {
                return Err(ViewError::OtherStream(body.tag().to_owned()));
            }
        }
        Ok(())
    }

    fn snapshot_ops(&self) -> Vec<EventBody> {
        self.snapshot_ops_within(&OpLimits::V1)
    }
}

impl SessionView {
    /// The turn still running: the newest one, when it has not completed.
    pub fn running_turn(&self) -> Option<&TurnView> {
        self.turns.last().filter(|t| t.completed.is_none())
    }

    pub fn pending_requests(&self) -> impl Iterator<Item = &RequestView> {
        self.requests.values().filter(|r| r.is_pending())
    }

    /// Drops the finished turns before the window (the host calls this when
    /// a turn ends). Their items go; their requests go too, except pending
    /// ones, which wait in `detached` until resolved. The running turn and
    /// the tail always stay.
    pub fn trim(&mut self, window: &Window) {
        let (mut turns, mut items) = (0, 0);
        let mut cut = None;
        for (i, turn) in self.turns.iter().enumerate().rev() {
            if turn.completed.is_none() {
                continue;
            }
            let n = turn
                .entries
                .iter()
                .filter(|e| matches!(e, Entry::Item(_)))
                .count();
            if turns > 0 && (turns >= window.turns || items + n > window.items) {
                cut = Some(i);
                break;
            }
            (turns, items) = (turns + 1, items + n);
        }
        let mut gone = BTreeSet::new();
        let evicted: Vec<TurnView> = match cut {
            Some(cut) => self
                .turns
                .extract_if(..=cut, |t| t.completed.is_some())
                .collect(),
            None => Vec::new(),
        };
        for entry in evicted.into_iter().flat_map(|t| t.entries) {
            match entry {
                Entry::Item(id) => {
                    self.items.remove(&id);
                    gone.insert(id);
                }
                Entry::Request(id)
                    if self.requests.get(&id).is_some_and(RequestView::is_pending) =>
                {
                    self.detached.push(id)
                }
                Entry::Request(id) => drop(self.requests.remove(&id)),
            }
        }
        self.late_by_ref.retain(|_, late| !gone.contains(late));
        let requests = &mut self.requests;
        self.detached.retain(|id| {
            let pending = requests.get(id).is_some_and(RequestView::is_pending);
            if !pending {
                requests.remove(id);
            }
            pending
        });
    }

    fn turn_index(&self, turn_id: &TurnId) -> Option<usize> {
        self.turns
            .iter()
            .rposition(|t| t.started.turn_id == *turn_id)
    }

    fn turn_mut(&mut self, turn_id: &TurnId) -> Result<&mut TurnView, ViewError> {
        let i = self.turn_index(turn_id).ok_or_else(|| missing(turn_id))?;
        Ok(&mut self.turns[i])
    }

    fn item_mut(&mut self, item_id: &ItemId) -> Result<&mut ItemView, ViewError> {
        self.items.get_mut(item_id).ok_or_else(|| missing(item_id))
    }

    /// Inserts a turn by `turn_no`. A turn newer than all others takes in the tail.
    fn start_turn(&mut self, started: &TurnStarted) -> Result<(), ViewError> {
        let same = |t: &TurnView| {
            t.started.turn_id == started.turn_id || t.started.turn_no == started.turn_no
        };
        if self.turns.iter().any(same) {
            return Err(ViewError::Duplicate(started.turn_id.to_string()));
        }
        let at = self
            .turns
            .partition_point(|t| t.started.turn_no < started.turn_no);
        let entries = if at == self.turns.len() {
            std::mem::take(&mut self.tail)
        } else {
            Vec::new()
        };
        for entry in &entries {
            self.set_turn(entry, &started.turn_id);
        }
        let turn = TurnView {
            started: started.clone(),
            completed: None,
            snapshots: Vec::new(),
            entries,
        };
        self.turns.insert(at, turn);
        Ok(())
    }

    fn set_turn(&mut self, entry: &Entry, turn_id: &TurnId) {
        let slot = match entry {
            Entry::Item(id) => self.items.get_mut(id).map(|i| &mut i.item.turn_id),
            Entry::Request(id) => self.requests.get_mut(id).map(|r| &mut r.requested.turn_id),
        };
        if let Some(slot) = slot {
            *slot = Some(turn_id.clone());
        }
    }

    /// Appends `entry` to its turn, or, without `turn_id`, to the running
    /// turn or the tail; returns the turn it joined.
    fn place(
        &mut self,
        entry: Entry,
        turn_id: Option<&TurnId>,
    ) -> Result<Option<TurnId>, ViewError> {
        let turn = match turn_id {
            Some(id) => Some(self.turn_index(id).ok_or_else(|| missing(id))?),
            None => self.running_turn().map(|_| self.turns.len() - 1),
        };
        let Some(i) = turn else {
            self.tail.push(entry);
            return Ok(None);
        };
        self.turns[i].entries.push(entry);
        Ok(Some(self.turns[i].started.turn_id.clone()))
    }

    fn start_item(&mut self, started: &ItemStarted) -> Result<(), ViewError> {
        let id = &started.item_id;
        if self.items.contains_key(id) {
            return Err(ViewError::Duplicate(id.to_string()));
        }
        let mut item = started.clone();
        item.turn_id = self.place(Entry::Item(id.clone()), started.turn_id.as_ref())?;
        if let Item::LateToolOutput(late) = &item.item {
            self.late_by_ref.insert(late.of.item_id.clone(), id.clone());
        }
        let view = ItemView {
            item,
            text: String::new(),
            output: String::new(),
            end: None,
        };
        self.items.insert(id.clone(), view);
        Ok(())
    }

    /// A request for a turn the view does not hold is detached; the turn's
    /// history page brings it again, which puts it in place and keeps the
    /// state it has reached since.
    fn request(&mut self, requested: &InteractionRequested) -> Result<(), ViewError> {
        let id = &requested.request_id;
        let turn = requested.turn_id.as_ref();
        let turn_index = turn.and_then(|t| self.turn_index(t));
        if self.requests.contains_key(id) {
            let detached = self.detached.iter().position(|d| d == id);
            let (Some(d), Some(t)) = (detached, turn_index) else {
                return Err(ViewError::Duplicate(id.to_string()));
            };
            self.detached.remove(d);
            self.turns[t].entries.push(Entry::Request(id.clone()));
            return Ok(());
        }
        let mut requested = requested.clone();
        if turn.is_some() && turn_index.is_none() {
            self.detached.push(id.clone());
        } else {
            requested.turn_id = self.place(Entry::Request(id.clone()), turn)?;
        }
        let view = RequestView {
            requested,
            resolved: None,
        };
        self.requests.insert(id.clone(), view);
        Ok(())
    }
}

fn missing(id: &impl ToString) -> ViewError {
    ViewError::Missing(id.to_string())
}

/// Sets the fields of `patch` that `item`'s kind has (design 5.3.11).
fn patch(item: &mut Item, patch: &ItemPatch) {
    macro_rules! set {
        ($item:ident: $($required:ident),*; $($optional:ident),*) => {{
            $(if let Some(v) = &patch.$required { $item.$required = v.clone(); })*
            $(if let Some(v) = &patch.$optional { $item.$optional = Some(v.clone()); })*
        }};
    }
    match item {
        Item::ToolCall(t) => set!(t: tool_kind, title, paths, output_state, extraction;
            query, url, command, cwd, exit_code, signal, output, output_tail, output_bytes,
            diffs, parent_item_id),
        Item::LateToolOutput(t) => {
            set!(t: output_state; output, output_tail, output_bytes, exit_code, signal)
        }
        Item::Plan(p) => set!(p: source; entries, markdown),
        Item::Notice(n) => set!(n: params;),
        Item::Unknown(member) => {
            if let Ok(Value::Object(fields)) = serde_json::to_value(patch) {
                member.0.extend(fields);
            }
        }
        Item::UserMessage(_) | Item::AgentMessage | Item::Thought | Item::Error(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::view::fixtures::*;

    /// Turn `n` with `items` agent messages, finished unless it is `running`.
    fn turn_with(n: u32, items: usize, running: bool) -> Vec<EventBody> {
        let mut ops = vec![turn(n)];
        ops.extend((0..items).map(|i| item(&format!("i_{n}x{i}"), n, "agent_message")));
        if !running {
            ops.push(end(n));
        }
        ops
    }

    fn turn_nos(view: &SessionView) -> Vec<u32> {
        view.turns.iter().map(|t| t.started.turn_no).collect()
    }

    #[test]
    fn unit_view_window_trims_finished_turns_and_keeps_the_running_one() {
        let mut view = SessionView::default();
        for n in 1..=25 {
            applied(&mut view, &turn_with(n, 1, false));
        }
        applied(&mut view, &turn_with(26, 3, true));
        view.trim(&Window::V1);
        assert_eq!(turn_nos(&view), (6..=26).collect::<Vec<_>>());
        assert_eq!(view.items.len(), 23);

        // Finished turns count until the next one would pass either limit;
        // the running turn neither counts nor goes.
        let mut view = SessionView::default();
        for (n, items) in [(1, 1), (2, 2), (3, 3), (4, 1)] {
            applied(&mut view, &turn_with(n, items, false));
        }
        applied(&mut view, &turn_with(5, 9, true));
        let small = SessionView::clone(&view);
        view.trim(&Window { turns: 3, items: 4 });
        assert_eq!(turn_nos(&view), [3, 4, 5]);
        assert!(!view.items.contains_key(&"i_2x0".parse().unwrap()));
        let mut view = small;
        view.trim(&Window { turns: 3, items: 0 });
        assert_eq!(
            turn_nos(&view),
            [4, 5],
            "the newest finished turn always stays"
        );
        assert_eq!(view.running_turn().unwrap().started.turn_no, 5);
        assert_eq!(rebuilt(&view, &OpLimits::V1), view);
    }

    #[test]
    fn unit_view_window_history_inserts_older_turns_in_order() {
        // The host saves each turn as it ends and keeps one turn; a notice
        // outside any turn waits in the tail.
        let (mut host, mut replay) = (SessionView::default(), SessionView::default());
        let mut saved = Vec::new();
        for n in 1..=4 {
            let mut ops = turn_with(n, 2, false);
            ops.insert(1, item(&format!("i_loose{n}"), 0, "notice"));
            applied(&mut replay, &ops);
            applied(&mut host, &ops);
            saved.push(
                host.turn_ops(&format!("t_{n}").parse().unwrap(), &OpLimits::V1)
                    .unwrap(),
            );
            host.trim(&Window {
                turns: 1,
                items: 100,
            });
        }
        let tail = [item("i_tail", 0, "notice")];
        applied(&mut host, &tail);
        applied(&mut replay, &tail);
        let mut client = rebuilt(&host, &OpLimits::V1);
        assert_eq!(turn_nos(&client), [4]);

        // History pages come newest first; each turn lands in front.
        for ops in saved[..3].iter().rev() {
            applied(&mut client, ops);
        }
        assert_eq!(turn_nos(&client), [1, 2, 3, 4]);
        assert_eq!(client, replay);
        let loose = &client.items[&"i_loose2".parse().unwrap()];
        assert_eq!(
            loose.item.turn_id,
            Some("t_2".parse().unwrap()),
            "joined the running turn"
        );
        assert_eq!(client.tail, [Entry::Item("i_tail".parse().unwrap())]);

        // A new turn takes in the tail; an older one, or a repeat, does not.
        applied(&mut client, &[turn(5)]);
        assert!(client.tail.is_empty());
        let notice = &client.items[&"i_tail".parse().unwrap()];
        assert_eq!(notice.item.turn_id, Some("t_5".parse().unwrap()));
        assert_eq!(
            client.turns[4].entries,
            [Entry::Item("i_tail".parse().unwrap())]
        );
        assert_eq!(
            client.apply(&turn(3)),
            Err(ViewError::Duplicate("t_3".into()))
        );
    }

    #[test]
    fn unit_view_window_keeps_pending_requests_of_evicted_turns() {
        let mut host = SessionView::default();
        applied(
            &mut host,
            &[
                turn(1),
                ask("r_1", 1),
                ask("r_2", 1),
                answer("r_2", "resolved"),
                end(1),
            ],
        );
        let saved = host
            .turn_ops(&"t_1".parse().unwrap(), &OpLimits::V1)
            .unwrap();
        applied(&mut host, &[turn(2), end(2)]);
        host.trim(&Window {
            turns: 1,
            items: 100,
        });
        assert_eq!(turn_nos(&host), [2]);
        assert_eq!(host.detached, ["r_1".parse::<RequestId>().unwrap()]);
        assert_eq!(
            host.requests.len(),
            1,
            "the resolved request left with its turn"
        );

        // The snapshot carries it detached; the turn's history page puts it
        // back in place without undoing what happened since.
        let mut client = rebuilt(&host, &OpLimits::V1);
        assert_eq!(client, host);
        applied(&mut client, &[answer("r_1", "cancelled")]);
        applied(&mut client, &saved);
        assert!(client.detached.is_empty());
        assert_eq!(
            client.turns[0].entries,
            [
                Entry::Request("r_1".parse().unwrap()),
                Entry::Request("r_2".parse().unwrap())
            ]
        );
        let r1 = &client.requests[&"r_1".parse().unwrap()];
        assert_eq!(*r1.status(), InteractionStatus::Cancelled);
        assert_eq!(
            client.apply(&ask("r_1", 1)),
            Err(ViewError::Duplicate("r_1".into()))
        );

        // Once answered, the next trim lets it go.
        applied(&mut host, &[answer("r_1", "resolved")]);
        host.trim(&Window {
            turns: 1,
            items: 100,
        });
        assert!(host.detached.is_empty() && host.requests.is_empty());
    }

    #[test]
    fn unit_view_late_by_ref_links_late_output_to_its_tool_call() {
        let mut view = SessionView::default();
        applied(
            &mut view,
            &[
                turn(1),
                item("i_1", 1, "tool_call"),
                done("i_1", "abandoned", json!({"may_still_run": true})),
                end(1),
            ],
        );
        applied(
            &mut view,
            &[
                turn(2),
                late("i_2", 2, "i_1", 1),
                delta("i_2", "output", "ready\n"),
                delta("i_2", "output", "bye\n"),
                done(
                    "i_2",
                    "completed",
                    json!({"exit_code": 0, "output_state": "complete", "params": {"attempt": 1}}),
                ),
            ],
        );
        applied(&mut view, &[end(2)]);
        let (i_1, i_2): (ItemId, ItemId) = ("i_1".parse().unwrap(), "i_2".parse().unwrap());
        assert_eq!(view.late_by_ref[&i_1], i_2);
        let late = &view.items[&i_2];
        assert_eq!(late.output, "ready\nbye\n");
        let Item::LateToolOutput(output) = &late.item.item else {
            panic!()
        };
        assert_eq!(
            (output.exit_code, output.output_state.as_str()),
            (Some(0), "complete")
        );
        assert!(view.items[&i_1].end.as_ref().unwrap().may_still_run);

        // The original turn leaves the window: the late item still names the
        // command it reports on, and the link waits for the original to load.
        let mut window = view.clone();
        window.trim(&Window {
            turns: 1,
            items: 100,
        });
        assert!(!window.items.contains_key(&i_1));
        assert_eq!(window.late_by_ref[&i_1], i_2);
        let Item::LateToolOutput(output) = &window.items[&i_2].item.item else {
            panic!()
        };
        assert_eq!(
            (output.of.title.as_str(), output.of.command.as_deref()),
            ("Run", Some("npm start"))
        );
        assert_eq!(rebuilt(&window, &OpLimits::V1), window);

        // With the late item gone too, the link goes; its turn's history brings it back.
        let saved = view
            .turn_ops(&"t_2".parse().unwrap(), &OpLimits::V1)
            .unwrap();
        applied(&mut window, &turn_with(3, 0, false));
        window.trim(&Window {
            turns: 1,
            items: 100,
        });
        assert!(window.late_by_ref.is_empty());
        applied(&mut window, &saved);
        assert_eq!(window.late_by_ref[&i_1], i_2);
    }
}
