//! The view a stream keeps: the protocol's index or session view (T014),
//! behind one type.

use cupper_protocol::event::EventBody;
use cupper_protocol::ids::TurnId;
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::{IndexView, SessionView, View, ViewError, Window};

#[derive(Debug, Clone, PartialEq)]
pub enum StreamView {
    Index(IndexView),
    Session(Box<SessionView>),
}

impl StreamView {
    pub fn empty(id: &StreamId) -> Self {
        match id {
            StreamId::Index => Self::Index(IndexView::default()),
            StreamId::Session(_) => Self::Session(Box::default()),
        }
    }

    /// Applies `body` the way the host does. A session view, after a
    /// `turn.completed`, hands the turn to `ended` (which saves its snapshot)
    /// and then drops the turns before its window (design 5.2.8). A body the
    /// view rejects changes nothing, here and on every replay alike.
    pub fn fold(
        &mut self,
        body: &EventBody,
        ended: &mut dyn FnMut(&SessionView, &TurnId),
    ) -> Result<(), ViewError> {
        match self {
            Self::Index(view) => view.apply(body),
            Self::Session(view) => {
                view.apply(body)?;
                if let EventBody::TurnCompleted(turn) = body {
                    ended(view, &turn.turn_id);
                    view.trim(&Window::V1);
                }
                Ok(())
            }
        }
    }

    /// Bodies that rebuild the view from empty: a `snapshot` subscription.
    pub fn snapshot_ops(&self) -> Vec<EventBody> {
        match self {
            Self::Index(view) => view.snapshot_ops(),
            Self::Session(view) => view.snapshot_ops(),
        }
    }

    /// The view's own JSON, which a checkpoint stores as is (T014).
    pub fn to_json(&self) -> serde_json::Result<String> {
        match self {
            Self::Index(view) => serde_json::to_string(view),
            Self::Session(view) => serde_json::to_string(view),
        }
    }

    pub fn from_json(id: &StreamId, json: &str) -> serde_json::Result<Self> {
        Ok(match id {
            StreamId::Index => Self::Index(serde_json::from_str(json)?),
            StreamId::Session(_) => Self::Session(serde_json::from_str(json)?),
        })
    }
}
