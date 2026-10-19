//! Session snapshot ops (design 5.3.3, 5.2.8): the bodies that rebuild a
//! [`SessionView`], or one of its turns, cut so that every op fits.

use super::session::{Entry, SessionView, TurnView, WorkspaceView};
use super::{OpLimits, op_bytes};
use crate::event::EventBody;
use crate::ids::TurnId;
use crate::item::{DeltaField, Item, ItemCompleted, ItemDelta, ItemPatch, ItemUpdated};
use crate::text::split_escaped;

impl SessionView {
    /// The window as ops: session state, detached requests, the turns in
    /// order, then the tail.
    pub fn snapshot_ops_within(&self, limits: &OpLimits) -> Vec<EventBody> {
        let workspace = self.workspace.clone().map(|w| match w {
            WorkspaceView::Preparing(w) => EventBody::WorkspacePreparing(w),
            WorkspaceView::Ready(w) => EventBody::WorkspaceReady(w),
            WorkspaceView::Failed(w) => EventBody::WorkspaceFailed(w),
        });
        let state = [
            self.configured.clone().map(EventBody::SessionConfigured),
            self.capabilities
                .clone()
                .map(EventBody::SessionCapabilities),
            workspace,
            self.queue.clone().map(EventBody::QueueUpdated),
            self.usage.clone().map(EventBody::UsageUpdated),
        ];
        let mut ops: Vec<EventBody> = state.into_iter().flatten().collect();
        for id in &self.detached {
            self.push_entry(&Entry::Request(id.clone()), limits, &mut ops);
        }
        for turn in &self.turns {
            self.push_turn(turn, limits, &mut ops);
        }
        for entry in &self.tail {
            self.push_entry(entry, limits, &mut ops);
        }
        ops
    }

    /// One turn as ops: what the host stores as the turn's snapshot when it
    /// ends and serves as history (design 5.2.8).
    pub fn turn_ops(&self, turn_id: &TurnId, limits: &OpLimits) -> Option<Vec<EventBody>> {
        let turn = self.turns.iter().find(|t| t.started.turn_id == *turn_id)?;
        let mut ops = Vec::new();
        self.push_turn(turn, limits, &mut ops);
        Some(ops)
    }

    fn push_turn(&self, turn: &TurnView, limits: &OpLimits, ops: &mut Vec<EventBody>) {
        ops.push(EventBody::TurnStarted(turn.started.clone()));
        let snapshots = turn.snapshots.iter().cloned();
        ops.extend(snapshots.map(EventBody::SnapshotRecorded));
        for entry in &turn.entries {
            self.push_entry(entry, limits, ops);
        }
        ops.extend(turn.completed.clone().map(EventBody::TurnCompleted));
    }

    /// An item: `item.started` with its current fields, its text as
    /// `item.delta`s, then `item.completed`. A request: `interaction.requested`,
    /// then `interaction.resolved`.
    fn push_entry(&self, entry: &Entry, limits: &OpLimits, ops: &mut Vec<EventBody>) {
        let item = match entry {
            Entry::Request(id) => {
                if let Some(r) = self.requests.get(id) {
                    ops.push(EventBody::InteractionRequested(r.requested.clone()));
                    ops.extend(r.resolved.clone().map(EventBody::InteractionResolved));
                }
                return;
            }
            Entry::Item(id) => match self.items.get(id) {
                Some(item) => item,
                None => return,
            },
        };
        let item_id = &item.item.item_id;
        let mut started = item.item.clone();
        let patches = if op_bytes(&EventBody::ItemStarted(started.clone())) > limits.op {
            split_off(&mut started.item)
        } else {
            Vec::new()
        };
        ops.push(EventBody::ItemStarted(started));
        ops.extend(patches.into_iter().map(|patch| {
            let item_id = item_id.clone();
            EventBody::ItemUpdated(ItemUpdated { item_id, patch })
        }));
        for (field, text) in [
            (DeltaField::Text, &item.text),
            (DeltaField::Output, &item.output),
        ] {
            if text.is_empty() {
                continue;
            }
            ops.extend(split_escaped(text, limits.delta).into_iter().map(|piece| {
                EventBody::ItemDelta(ItemDelta {
                    item_id: item_id.clone(),
                    field: field.clone(),
                    text: piece.to_owned(),
                })
            }));
        }
        if let Some(end) = &item.end {
            ops.push(EventBody::ItemCompleted(ItemCompleted {
                item_id: item_id.clone(),
                status: end.status.clone(),
                ended_at: end.ended_at,
                duration_ms: end.duration_ms,
                may_still_run: end.may_still_run,
                patch: ItemPatch::default(),
            }));
        }
    }
}

/// Takes the optional fields out of `item`, one patch each. Every one of
/// them came in a single live event, so each patch fits a frame.
fn split_off(item: &mut Item) -> Vec<ItemPatch> {
    let mut patches = Vec::new();
    macro_rules! take {
        ($item:ident: $($field:ident),+) => {{$(
            if let Some(value) = $item.$field.take() {
                patches.push(ItemPatch { $field: Some(value), ..ItemPatch::default() });
            }
        )+}};
    }
    match item {
        Item::ToolCall(t) => take!(t: query, url, command, cwd, exit_code, signal, output,
            output_tail, output_bytes, diffs, parent_item_id),
        Item::LateToolOutput(t) => take!(t: output, output_tail, output_bytes, exit_code, signal),
        Item::Plan(p) => take!(p: entries, markdown),
        _ => {}
    }
    patches
}
