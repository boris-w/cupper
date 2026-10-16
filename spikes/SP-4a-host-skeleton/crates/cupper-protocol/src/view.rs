//! Pure stream state ("views") built by folding events.
//!
//! The host keeps one view per stream as its projection and serves snapshots
//! from it; clients fold the same events into the same view. Snapshots are
//! expressed as a list of event bodies (`to_ops`), so applying a snapshot and
//! then the following events uses exactly one `apply` implementation.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::events::{
    Content, EventBody, InteractionOutcome, InteractionRequest, InteractionRequested, InteractionResolved,
    InteractionStatus, ItemCompleted, ItemDelta, ItemKind, ItemStarted, ItemStatus, ItemUpdated, PrincipalKind,
    SessionSummary, TurnCompleted, TurnStarted, TurnStatus,
};
use crate::text::{MAX_DELTA_ESCAPED, split_escaped};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ViewError {
    #[error("unknown turn {0}")]
    UnknownTurn(String),
    #[error("unknown item {0}")]
    UnknownItem(String),
    #[error("unknown interaction {0}")]
    UnknownInteraction(String),
    #[error("duplicate id {0}")]
    Duplicate(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexView {
    pub sessions: Vec<SessionSummary>,
}

impl IndexView {
    pub fn apply(&mut self, body: &EventBody) -> Result<(), ViewError> {
        if let EventBody::SessionUpsert(s) = body {
            match self.sessions.iter_mut().find(|x| x.session_id == s.session_id) {
                Some(existing) => *existing = s.clone(),
                None => self.sessions.push(s.clone()),
            }
        }
        Ok(())
    }

    pub fn to_ops(&self) -> Vec<EventBody> {
        self.sessions.iter().cloned().map(EventBody::SessionUpsert).collect()
    }

    pub fn session(&self, id: &str) -> Option<&SessionSummary> {
        self.sessions.iter().find(|s| s.session_id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnState {
    pub turn_id: String,
    pub status: TurnStatus,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemState {
    pub item_id: String,
    pub turn_id: String,
    pub kind: ItemKind,
    pub title: Option<String>,
    pub tool_kind: Option<String>,
    pub status: ItemStatus,
    pub text: String,
    pub content: Option<Content>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionState {
    pub request_id: String,
    pub turn_id: String,
    pub item_id: Option<String>,
    pub digest: String,
    pub actionable_by: Vec<PrincipalKind>,
    pub request: InteractionRequest,
    pub status: InteractionStatus,
    pub outcome: Option<InteractionOutcome>,
    pub resolved_by: Option<String>,
}

/// State of one session stream. Unknown event types and notices do not change it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionView {
    pub turns: Vec<TurnState>,
    pub items: Vec<ItemState>,
    pub interactions: Vec<InteractionState>,
}

impl SessionView {
    pub fn apply(&mut self, body: &EventBody) -> Result<(), ViewError> {
        match body {
            EventBody::TurnStarted(t) => {
                if self.turn(&t.turn_id).is_some() {
                    return Err(ViewError::Duplicate(t.turn_id.clone()));
                }
                self.turns.push(TurnState { turn_id: t.turn_id.clone(), status: TurnStatus::Running, stop_reason: None });
            }
            EventBody::TurnCompleted(t) => {
                let turn = self.turn_mut(&t.turn_id)?;
                turn.status = t.status.clone();
                turn.stop_reason = t.stop_reason.clone();
            }
            EventBody::ItemStarted(i) => {
                if self.item(&i.item_id).is_some() {
                    return Err(ViewError::Duplicate(i.item_id.clone()));
                }
                self.items.push(ItemState {
                    item_id: i.item_id.clone(),
                    turn_id: i.turn_id.clone(),
                    kind: i.kind.clone(),
                    title: i.title.clone(),
                    tool_kind: i.tool_kind.clone(),
                    status: ItemStatus::Running,
                    text: String::new(),
                    content: None,
                });
            }
            EventBody::ItemDelta(d) => self.item_mut(&d.item_id)?.text.push_str(&d.text),
            EventBody::ItemUpdated(u) => {
                let item = self.item_mut(&u.item_id)?;
                if let Some(t) = &u.title {
                    item.title = Some(t.clone());
                }
                if let Some(c) = &u.content {
                    item.content = Some(c.clone());
                }
            }
            EventBody::ItemCompleted(c) => self.item_mut(&c.item_id)?.status = c.status.clone(),
            EventBody::InteractionRequested(r) => {
                if self.interaction(&r.request_id).is_some() {
                    return Err(ViewError::Duplicate(r.request_id.clone()));
                }
                self.interactions.push(InteractionState {
                    request_id: r.request_id.clone(),
                    turn_id: r.turn_id.clone(),
                    item_id: r.item_id.clone(),
                    digest: r.digest.clone(),
                    actionable_by: r.actionable_by.clone(),
                    request: r.request.clone(),
                    status: InteractionStatus::Pending,
                    outcome: None,
                    resolved_by: None,
                });
            }
            EventBody::InteractionResolved(r) => {
                let i = self
                    .interactions
                    .iter_mut()
                    .rev()
                    .find(|x| x.request_id == r.request_id)
                    .ok_or_else(|| ViewError::UnknownInteraction(r.request_id.clone()))?;
                i.status = r.status.clone();
                i.outcome = r.outcome.clone();
                i.resolved_by = r.resolved_by.clone();
            }
            EventBody::SessionUpsert(_) | EventBody::AgentNotice(_) | EventBody::Unknown(_) => {}
        }
        Ok(())
    }

    /// Event bodies that rebuild this view from empty. Long texts are split so
    /// every op stays within the frame budget.
    pub fn to_ops(&self) -> Vec<EventBody> {
        let mut ops = Vec::new();
        for t in &self.turns {
            ops.push(EventBody::TurnStarted(TurnStarted { turn_id: t.turn_id.clone() }));
        }
        for i in &self.items {
            ops.push(EventBody::ItemStarted(ItemStarted {
                item_id: i.item_id.clone(),
                turn_id: i.turn_id.clone(),
                kind: i.kind.clone(),
                title: i.title.clone(),
                tool_kind: i.tool_kind.clone(),
            }));
            if !i.text.is_empty() {
                for piece in split_escaped(&i.text, MAX_DELTA_ESCAPED) {
                    ops.push(EventBody::ItemDelta(ItemDelta { item_id: i.item_id.clone(), text: piece.to_owned() }));
                }
            }
            if i.content.is_some() {
                ops.push(EventBody::ItemUpdated(ItemUpdated {
                    item_id: i.item_id.clone(),
                    title: None,
                    content: i.content.clone(),
                }));
            }
            if i.status != ItemStatus::Running {
                ops.push(EventBody::ItemCompleted(ItemCompleted { item_id: i.item_id.clone(), status: i.status.clone() }));
            }
        }
        for r in &self.interactions {
            ops.push(EventBody::InteractionRequested(InteractionRequested {
                request_id: r.request_id.clone(),
                turn_id: r.turn_id.clone(),
                item_id: r.item_id.clone(),
                digest: r.digest.clone(),
                actionable_by: r.actionable_by.clone(),
                request: r.request.clone(),
            }));
            if r.status != InteractionStatus::Pending {
                ops.push(EventBody::InteractionResolved(InteractionResolved {
                    request_id: r.request_id.clone(),
                    status: r.status.clone(),
                    outcome: r.outcome.clone(),
                    resolved_by: r.resolved_by.clone(),
                }));
            }
        }
        for t in &self.turns {
            if t.status != TurnStatus::Running {
                ops.push(EventBody::TurnCompleted(TurnCompleted {
                    turn_id: t.turn_id.clone(),
                    status: t.status.clone(),
                    stop_reason: t.stop_reason.clone(),
                }));
            }
        }
        ops
    }

    pub fn turn(&self, id: &str) -> Option<&TurnState> {
        self.turns.iter().rev().find(|t| t.turn_id == id)
    }

    fn turn_mut(&mut self, id: &str) -> Result<&mut TurnState, ViewError> {
        self.turns.iter_mut().rev().find(|t| t.turn_id == id).ok_or_else(|| ViewError::UnknownTurn(id.to_owned()))
    }

    pub fn item(&self, id: &str) -> Option<&ItemState> {
        self.items.iter().rev().find(|i| i.item_id == id)
    }

    fn item_mut(&mut self, id: &str) -> Result<&mut ItemState, ViewError> {
        self.items.iter_mut().rev().find(|i| i.item_id == id).ok_or_else(|| ViewError::UnknownItem(id.to_owned()))
    }

    pub fn interaction(&self, id: &str) -> Option<&InteractionState> {
        self.interactions.iter().rev().find(|i| i.request_id == id)
    }

    pub fn running_turn(&self) -> Option<&TurnState> {
        self.turns.iter().rev().find(|t| t.status == TurnStatus::Running)
    }

    pub fn pending_interactions(&self) -> impl Iterator<Item = &InteractionState> {
        self.interactions.iter().filter(|i| i.status == InteractionStatus::Pending)
    }

    /// Concatenated text of all agent message items, handy for tests and the CLI.
    pub fn agent_text(&self) -> String {
        self.items.iter().filter(|i| i.kind == ItemKind::AgentMessage).map(|i| i.text.as_str()).collect()
    }
}

/// View of either stream kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum StreamView {
    Index(IndexView),
    Session(SessionView),
}

impl StreamView {
    pub fn empty_for(stream: &crate::sync::StreamId) -> Self {
        match stream {
            crate::sync::StreamId::Index => StreamView::Index(IndexView::default()),
            crate::sync::StreamId::Session(_) => StreamView::Session(SessionView::default()),
        }
    }

    pub fn apply(&mut self, body: &EventBody) -> Result<(), ViewError> {
        match self {
            StreamView::Index(v) => v.apply(body),
            StreamView::Session(v) => v.apply(body),
        }
    }

    pub fn to_ops(&self) -> Vec<EventBody> {
        match self {
            StreamView::Index(v) => v.to_ops(),
            StreamView::Session(v) => v.to_ops(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Content, PermissionOption, PermissionOptionKind};

    #[test]
    fn snapshot_ops_rebuild_the_same_view() {
        let mut v = SessionView::default();
        let ops = vec![
            EventBody::TurnStarted(TurnStarted { turn_id: "t1".into() }),
            EventBody::ItemStarted(ItemStarted {
                item_id: "i1".into(),
                turn_id: "t1".into(),
                kind: ItemKind::AgentMessage,
                title: None,
                tool_kind: None,
            }),
            EventBody::ItemDelta(ItemDelta { item_id: "i1".into(), text: "x".repeat(50_000) }),
            EventBody::ItemStarted(ItemStarted {
                item_id: "i2".into(),
                turn_id: "t1".into(),
                kind: ItemKind::ToolCall,
                title: Some("Write".into()),
                tool_kind: Some("edit".into()),
            }),
            EventBody::ItemUpdated(ItemUpdated {
                item_id: "i2".into(),
                title: Some("Write a.txt".into()),
                content: Some(Content::Text { text: "diff".into() }),
            }),
            EventBody::InteractionRequested(InteractionRequested {
                request_id: "r1".into(),
                turn_id: "t1".into(),
                item_id: Some("i2".into()),
                digest: "d".into(),
                actionable_by: vec![PrincipalKind::DesktopUi],
                request: InteractionRequest::Permission {
                    title: "Write".into(),
                    options: vec![PermissionOption {
                        option_id: "o".into(),
                        name: "Allow".into(),
                        kind: PermissionOptionKind::AllowOnce,
                    }],
                    detail: None,
                },
            }),
            EventBody::InteractionResolved(InteractionResolved {
                request_id: "r1".into(),
                status: InteractionStatus::Resolved,
                outcome: Some(InteractionOutcome::Selected { option_id: "o".into() }),
                resolved_by: Some("p".into()),
            }),
            EventBody::ItemCompleted(ItemCompleted { item_id: "i2".into(), status: ItemStatus::Completed }),
            EventBody::TurnCompleted(TurnCompleted {
                turn_id: "t1".into(),
                status: TurnStatus::Completed,
                stop_reason: Some("end_turn".into()),
            }),
        ];
        for op in &ops {
            v.apply(op).unwrap();
        }
        let snapshot = v.to_ops();
        assert!(snapshot.iter().filter(|o| matches!(o, EventBody::ItemDelta(_))).count() >= 4);
        let mut rebuilt = SessionView::default();
        for op in &snapshot {
            rebuilt.apply(op).unwrap();
        }
        assert_eq!(rebuilt, v);
    }
}
