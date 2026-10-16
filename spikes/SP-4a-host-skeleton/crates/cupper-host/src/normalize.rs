//! Turns driver updates into log events for one turn, including write-time
//! coalescing of text fragments.
//!
//! Rules (ADR-04): the first fragment of an item goes out immediately; later
//! fragments are buffered for a short window and flushed when the window ends,
//! when the item ends, or before any other event. Everything that could grow
//! large is split or turned into a blob reference so no event exceeds the
//! frame budget.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use cupper_protocol::text::{MAX_DELTA_ESCAPED, MAX_INLINE_ESCAPED, MAX_LABEL_CHARS, PREVIEW_CHARS, escaped_len, split_escaped, truncate_chars};
use cupper_protocol::{
    AgentNotice, BlobRef, Content, EventBody, ItemCompleted, ItemDelta, ItemKind, ItemStarted, ItemStatus, ItemUpdated,
};

use crate::driver::{NormUpdate, ToolState};
use crate::util::new_id;

/// Where large content goes. Returns a reference to the stored bytes.
pub trait BlobSink {
    fn put(&mut self, bytes: &[u8]) -> BlobRef;
}

/// Inline content if small, otherwise a blob reference with a preview.
pub fn content(text: String, blobs: &mut dyn BlobSink) -> Content {
    if escaped_len(&text) <= MAX_INLINE_ESCAPED {
        Content::Text { text }
    } else {
        let blob = blobs.put(text.as_bytes());
        Content::Blob { blob, preview: truncate_chars(&text, PREVIEW_CHARS) }
    }
}

fn label(s: &str) -> String {
    truncate_chars(s, MAX_LABEL_CHARS)
}

#[derive(Debug)]
struct OpenText {
    item_id: String,
    kind: ItemKind,
    message_id: Option<String>,
}

#[derive(Debug)]
pub struct Normalizer {
    turn_id: String,
    window: Duration,
    open_text: Option<OpenText>,
    /// ACP tool call id -> (item id, reached a terminal status).
    tools: HashMap<String, (String, bool)>,
    plan_item: Option<String>,
    /// Buffered text for the open text item, and when buffering started.
    pending: String,
    pending_since: Option<Instant>,
}

impl Normalizer {
    pub fn new(turn_id: String, window: Duration) -> Self {
        Self {
            turn_id,
            window,
            open_text: None,
            tools: HashMap::new(),
            plan_item: None,
            pending: String::new(),
            pending_since: None,
        }
    }

    /// When the buffered text must be flushed.
    pub fn deadline(&self) -> Option<Instant> {
        self.pending_since.map(|t| t + self.window)
    }

    /// Emit buffered text, split to the delta budget.
    pub fn flush(&mut self) -> Vec<EventBody> {
        self.pending_since = None;
        let (Some(open), false) = (&self.open_text, self.pending.is_empty()) else {
            return Vec::new();
        };
        let text = std::mem::take(&mut self.pending);
        split_escaped(&text, MAX_DELTA_ESCAPED)
            .into_iter()
            .map(|p| EventBody::ItemDelta(ItemDelta { item_id: open.item_id.clone(), text: p.to_owned() }))
            .collect()
    }

    fn close_text(&mut self, status: ItemStatus) -> Vec<EventBody> {
        let mut out = self.flush();
        if let Some(open) = self.open_text.take() {
            out.push(EventBody::ItemCompleted(ItemCompleted { item_id: open.item_id, status }));
        }
        out
    }

    fn text(&mut self, kind: ItemKind, message_id: Option<String>, text: String, now: Instant) -> Vec<EventBody> {
        let continues = self.open_text.as_ref().is_some_and(|o| o.kind == kind && o.message_id == message_id);
        if continues {
            self.pending.push_str(&text);
            if self.pending_since.is_none() {
                self.pending_since = Some(now);
            }
            return if escaped_len(&self.pending) >= MAX_DELTA_ESCAPED { self.flush() } else { Vec::new() };
        }
        let mut out = self.close_text(ItemStatus::Completed);
        let item_id = new_id("i");
        out.push(EventBody::ItemStarted(ItemStarted {
            item_id: item_id.clone(),
            turn_id: self.turn_id.clone(),
            kind: kind.clone(),
            title: None,
            tool_kind: None,
        }));
        self.open_text = Some(OpenText { item_id, kind, message_id });
        // The first fragment of an item goes out immediately.
        self.pending = text;
        out.extend(self.flush());
        out
    }

    /// `item.completed` when a tool call reaches a terminal status (once).
    fn tool_status(&mut self, tool_call_id: &str, status: Option<ToolState>) -> Option<EventBody> {
        let status = match status? {
            ToolState::Running => return None,
            ToolState::Completed => ItemStatus::Completed,
            ToolState::Failed => ItemStatus::Failed,
        };
        let (item_id, done) = self.tools.get_mut(tool_call_id)?;
        if *done {
            return None;
        }
        *done = true;
        Some(EventBody::ItemCompleted(ItemCompleted { item_id: item_id.clone(), status }))
    }

    pub fn on_update(&mut self, update: NormUpdate, now: Instant, blobs: &mut dyn BlobSink) -> Vec<EventBody> {
        match update {
            NormUpdate::AgentText { message_id, text } => self.text(ItemKind::AgentMessage, message_id, text, now),
            NormUpdate::Thought { text } => self.text(ItemKind::Thought, None, text, now),
            NormUpdate::ToolCall { id, title, kind, status, content: body } => {
                let mut out = self.close_text(ItemStatus::Completed);
                let item_id = new_id("i");
                self.tools.insert(id.clone(), (item_id.clone(), false));
                out.push(EventBody::ItemStarted(ItemStarted {
                    item_id: item_id.clone(),
                    turn_id: self.turn_id.clone(),
                    kind: ItemKind::ToolCall,
                    title: Some(label(&title)),
                    tool_kind: Some(label(&kind)),
                }));
                if let Some(b) = body {
                    out.push(EventBody::ItemUpdated(ItemUpdated { item_id: item_id.clone(), title: None, content: Some(content(b, blobs)) }));
                }
                out.extend(self.tool_status(&id, status));
                out
            }
            NormUpdate::ToolUpdate { id, title, status, content: body } => {
                let mut out = self.close_text(ItemStatus::Completed);
                let item_id = match self.tools.get(&id) {
                    Some((i, _)) => i.clone(),
                    None => {
                        let item_id = new_id("i");
                        self.tools.insert(id.clone(), (item_id.clone(), false));
                        out.push(EventBody::ItemStarted(ItemStarted {
                            item_id: item_id.clone(),
                            turn_id: self.turn_id.clone(),
                            kind: ItemKind::ToolCall,
                            title: Some(label(title.as_deref().unwrap_or(&id))),
                            tool_kind: None,
                        }));
                        item_id
                    }
                };
                if title.is_some() || body.is_some() {
                    out.push(EventBody::ItemUpdated(ItemUpdated {
                        item_id: item_id.clone(),
                        title: title.as_deref().map(label),
                        content: body.map(|b| content(b, blobs)),
                    }));
                }
                out.extend(self.tool_status(&id, status));
                out
            }
            NormUpdate::Plan { text } => {
                let mut out = self.close_text(ItemStatus::Completed);
                let item_id = match &self.plan_item {
                    Some(i) => i.clone(),
                    None => {
                        let i = new_id("i");
                        out.push(EventBody::ItemStarted(ItemStarted {
                            item_id: i.clone(),
                            turn_id: self.turn_id.clone(),
                            kind: ItemKind::Plan,
                            title: None,
                            tool_kind: None,
                        }));
                        self.plan_item = Some(i.clone());
                        i
                    }
                };
                out.push(EventBody::ItemUpdated(ItemUpdated { item_id, title: None, content: Some(content(text, blobs)) }));
                out
            }
            NormUpdate::Other { kind } => {
                let mut out = self.flush();
                out.push(EventBody::AgentNotice(AgentNotice { kind: label(&kind), summary: None }));
                out
            }
        }
    }

    /// Something else is about to be logged (e.g. an interaction request):
    /// flush buffered text first so ordering is preserved.
    pub fn before_other_event(&mut self) -> Vec<EventBody> {
        self.flush()
    }

    /// The tool call item for an ACP tool call id, if known.
    pub fn tool_item(&self, tool_call_id: &str) -> Option<String> {
        self.tools.get(tool_call_id).map(|(i, _)| i.clone())
    }

    /// Close everything still open at the end of the turn.
    pub fn finish(&mut self, cancelled: bool) -> Vec<EventBody> {
        let mut out = self.close_text(ItemStatus::Completed);
        let status = if cancelled { ItemStatus::Cancelled } else { ItemStatus::Completed };
        let mut open: Vec<String> = self.tools.drain().filter(|(_, (_, done))| !done).map(|(_, (item, _))| item).collect();
        open.extend(self.plan_item.take());
        open.sort();
        for item_id in open {
            out.push(EventBody::ItemCompleted(ItemCompleted { item_id, status: status.clone() }));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cupper_protocol::coalesce::coalesce;
    use cupper_protocol::frame::MAX_FRAME_BYTES;
    use cupper_protocol::text::MAX_MERGED_ESCAPED;
    use cupper_protocol::view::SessionView;
    use cupper_protocol::{EVENT_VERSION, Event};
    use proptest::prelude::*;

    struct FakeBlobs;
    impl BlobSink for FakeBlobs {
        fn put(&mut self, bytes: &[u8]) -> BlobRef {
            BlobRef { hash: "0".repeat(64), size: bytes.len() as u64, media_type: None }
        }
    }

    /// Text that is sometimes huge and sometimes escape-heavy (control chars
    /// grow 6x when JSON-escaped).
    fn text() -> impl Strategy<Value = String> {
        prop_oneof![
            "\\PC{0,50}",
            (1usize..300_000, any::<char>()).prop_map(|(n, c)| std::iter::repeat_n(c, n).collect()),
            (1usize..60_000).prop_map(|n| "\u{1}\"\n".repeat(n)),
        ]
    }

    fn update() -> impl Strategy<Value = NormUpdate> {
        let id = (0u8..4).prop_map(|i| format!("tc{i}"));
        prop_oneof![
            4 => (prop::option::of("m[0-2]"), text()).prop_map(|(message_id, text)| NormUpdate::AgentText { message_id, text }),
            2 => text().prop_map(|text| NormUpdate::Thought { text }),
            1 => (id.clone(), text(), prop::option::of(text())).prop_map(|(id, title, content)| NormUpdate::ToolCall {
                id, title, kind: "edit".into(), status: Some(ToolState::Running), content,
            }),
            1 => (id, prop::option::of(text()), prop::option::of(text())).prop_map(|(id, title, content)| NormUpdate::ToolUpdate {
                id, title, status: Some(ToolState::Completed), content,
            }),
            1 => text().prop_map(|text| NormUpdate::Plan { text }),
            1 => text().prop_map(|kind| NormUpdate::Other { kind }),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// SP-4a test 7: whatever the agent emits, every frame the host sends
        /// (live events after send-time coalescing, and snapshot pages) fits the
        /// 64,000-byte limit, and the snapshot rebuilds the same state.
        #[test]
        fn every_frame_fits_the_limit(updates in prop::collection::vec(update(), 1..25)) {
            let mut n = Normalizer::new("t_1".into(), Duration::from_millis(40));
            let now = Instant::now();
            let mut bodies = vec![EventBody::TurnStarted(cupper_protocol::TurnStarted { turn_id: "t_1".into() })];
            for u in updates {
                bodies.extend(n.on_update(u, now, &mut FakeBlobs));
            }
            bodies.extend(n.finish(false));
            let events: Vec<Event> = bodies
                .into_iter()
                .enumerate()
                .map(|(i, body)| Event { seq: i as u64 + 1, from: None, v: EVENT_VERSION, ts: 0, cause: None, fallback: None, body })
                .collect();
            let mut view = SessionView::default();
            for e in &events {
                view.apply(&e.body).unwrap();
            }
            let jsons: Vec<String> = coalesce(events, MAX_MERGED_ESCAPED).iter().map(|e| serde_json::to_string(e).unwrap()).collect();
            for f in crate::wire::events("session/s_x", &jsons).unwrap() {
                prop_assert!(f.len() <= MAX_FRAME_BYTES, "event frame of {} bytes", f.len());
            }
            let ops = view.to_ops();
            for f in crate::wire::snapshot("session/s_x", 1, &ops).unwrap() {
                prop_assert!(f.len() <= MAX_FRAME_BYTES, "snapshot frame of {} bytes", f.len());
            }
            let mut rebuilt = SessionView::default();
            for op in &ops {
                rebuilt.apply(op).unwrap();
            }
            prop_assert_eq!(rebuilt, view);
        }
    }
}
