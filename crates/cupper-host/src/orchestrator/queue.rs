//! `QueueCore` (design 5.2.6, CP-6): the messages sent while a turn runs, in
//! arrival order. A pure core with no timers; its only caller is the
//! `TurnMachine`, which steps it within its own step and decides when to
//! pause it and when to take the head as a new turn. Every change returns the
//! whole queue as the `queue.updated` to write, with a new version.

use std::collections::VecDeque;

use cupper_protocol::domain::{PromptBlock, PromptContent};
use cupper_protocol::ids::QueueId;
use cupper_protocol::limits::V1;
use cupper_protocol::session::{PauseReason, QueueEntry, QueuePause, QueueUpdated};
use thiserror::Error;

#[derive(Debug, Clone, Default)]
pub struct QueueCore {
    version: u64,
    paused: Option<PauseReason>,
    entries: VecDeque<QueueEntry>,
}

/// One operation of `session/queue_edit`.
#[derive(Debug, Clone, PartialEq)]
pub enum QueueOp {
    Replace {
        queue_id: QueueId,
        content: PromptContent,
    },
    /// Steer this message into the running turn; only on its own.
    Steer { queue_id: QueueId },
    /// CP-8 (M2).
    Remove { queue_id: QueueId },
    /// CP-8 (M2).
    Move {
        queue_id: QueueId,
        before: Option<QueueId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the queue already holds the most messages a session may queue")]
pub struct QueueFull;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QueueEditError {
    #[error("the queue is at version {current}")]
    VersionMismatch { current: u64 },
    #[error("{0} is not in the queue")]
    NotFound(QueueId),
    #[error("a steer must be the only operation")]
    SteerNotAlone,
    #[error("removing and moving queued messages come with CP-8")]
    Unsupported,
}

impl QueueCore {
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn push(&mut self, entry: QueueEntry) -> Result<QueueUpdated, QueueFull> {
        if self.entries.len() >= V1.max_queued_messages as usize {
            return Err(QueueFull);
        }
        self.entries.push_back(entry);
        Ok(self.updated())
    }

    /// Takes the head, unless the queue is paused or empty.
    pub fn pop(&mut self) -> Option<(QueueEntry, QueueUpdated)> {
        if self.paused.is_some() {
            return None;
        }
        let entry = self.entries.pop_front()?;
        Some((entry, self.updated()))
    }

    /// Holds the queue back. Nothing changes when it is already paused, or
    /// empty: no message would wait.
    pub fn pause(&mut self, reason: PauseReason) -> Option<QueueUpdated> {
        if self.paused.is_some() || self.entries.is_empty() {
            return None;
        }
        self.paused = Some(reason);
        Some(self.updated())
    }

    pub fn resume(&mut self) -> Option<QueueUpdated> {
        self.paused.take()?;
        Some(self.updated())
    }

    /// Applies `ops` at once or not at all. Returns the changed queue, or
    /// `None` when nothing changed (a steer leaves its message in place).
    pub fn edit(
        &mut self,
        expected_version: u64,
        ops: &[QueueOp],
    ) -> Result<Option<QueueUpdated>, QueueEditError> {
        if expected_version != self.version {
            return Err(QueueEditError::VersionMismatch {
                current: self.version,
            });
        }
        let mut replaced = self.entries.clone();
        for op in ops {
            let queue_id = match op {
                QueueOp::Replace { queue_id, .. } => queue_id,
                QueueOp::Steer { queue_id } if ops.len() == 1 => queue_id,
                QueueOp::Steer { .. } => return Err(QueueEditError::SteerNotAlone),
                QueueOp::Remove { .. } | QueueOp::Move { .. } => {
                    return Err(QueueEditError::Unsupported);
                }
            };
            let entry = replaced
                .iter_mut()
                .find(|entry| entry.queue_id == *queue_id)
                .ok_or_else(|| QueueEditError::NotFound(queue_id.clone()))?;
            if let QueueOp::Replace { content, .. } = op {
                entry.preview = preview(content);
                entry.content = content.clone();
            }
        }
        if replaced == self.entries {
            return Ok(None);
        }
        self.entries = replaced;
        Ok(Some(self.updated()))
    }

    fn updated(&mut self) -> QueueUpdated {
        self.version += 1;
        QueueUpdated {
            version: self.version,
            paused: self.paused.clone().map(|reason| QueuePause { reason }),
            entries: self.entries.iter().cloned().collect(),
        }
    }
}

/// A queued message as the queue lists it: its text blocks, cut to the
/// label limit.
pub fn preview(content: &PromptContent) -> String {
    let texts: Vec<&str> = content
        .iter()
        .filter_map(|block| match block {
            PromptBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    texts
        .join(" ")
        .chars()
        .take(V1.max_label_chars as usize)
        .collect()
}

#[cfg(test)]
mod tests {
    use cupper_protocol::annotations::{TimestampMs, WirePath};
    use cupper_protocol::domain::{FileRef, TextContent};

    use super::*;

    fn text(text: &str) -> PromptContent {
        vec![PromptBlock::Text(TextContent { text: text.into() })]
    }

    fn entry(n: u128) -> QueueEntry {
        QueueEntry {
            queue_id: QueueId::from_uuid(n),
            preview: format!("m{n}"),
            content: text(&format!("m{n}")),
            reply_to: None,
            created_at: TimestampMs(0),
            created_by: "pr_test".parse().unwrap(),
        }
    }

    fn ids(update: &QueueUpdated) -> Vec<QueueId> {
        update.entries.iter().map(|e| e.queue_id.clone()).collect()
    }

    #[test]
    fn queue_core_pauses_only_with_waiting_messages_and_counts_versions() {
        let mut queue = QueueCore::default();
        assert_eq!(queue.pause(PauseReason::TurnCancelled), None);
        assert_eq!(queue.resume(), None);
        assert_eq!(queue.pop(), None);
        queue.push(entry(1)).unwrap();
        let update = queue.push(entry(2)).unwrap();
        assert_eq!((update.version, ids(&update).len()), (2, 2));

        let paused = queue.pause(PauseReason::TurnFailed).unwrap();
        assert_eq!(paused.paused.unwrap().reason, PauseReason::TurnFailed);
        assert_eq!(queue.pause(PauseReason::TurnCancelled), None);
        assert_eq!(queue.pop(), None);
        assert_eq!(queue.resume().unwrap().paused, None);
        let (head, update) = queue.pop().unwrap();
        assert_eq!(head, entry(1));
        assert_eq!(
            (update.version, ids(&update)),
            (5, vec![QueueId::from_uuid(2)])
        );

        for n in 3..=u128::from(V1.max_queued_messages) + 1 {
            queue.push(entry(n)).unwrap();
        }
        assert_eq!(queue.push(entry(99)), Err(QueueFull));
    }

    #[test]
    fn queue_core_edit_checks_version_then_applies_all_or_nothing() {
        let mut queue = QueueCore::default();
        queue.push(entry(1)).unwrap();
        queue.push(entry(2)).unwrap();
        let replace = |n, to: &str| QueueOp::Replace {
            queue_id: QueueId::from_uuid(n),
            content: text(to),
        };
        let steer = QueueOp::Steer {
            queue_id: QueueId::from_uuid(2),
        };
        let err = |ops: &[QueueOp], version| queue_err(&mut queue.clone(), version, ops);
        assert_eq!(
            err(&[replace(1, "a")], 1),
            QueueEditError::VersionMismatch { current: 2 }
        );
        assert_eq!(
            err(&[replace(1, "a"), replace(7, "b")], 2),
            QueueEditError::NotFound(QueueId::from_uuid(7))
        );
        assert_eq!(
            err(&[replace(1, "a"), steer.clone()], 2),
            QueueEditError::SteerNotAlone
        );
        let remove = QueueOp::Remove {
            queue_id: QueueId::from_uuid(1),
        };
        assert_eq!(err(&[remove], 2), QueueEditError::Unsupported);

        assert_eq!(queue.edit(2, &[steer]), Ok(None));
        let update = queue.edit(2, &[replace(2, "new")]).unwrap().unwrap();
        assert_eq!(update.version, 3);
        assert_eq!(update.entries[1].content, text("new"));
        assert_eq!(update.entries[1].preview, "new");
        assert_eq!(update.entries[0], entry(1));
    }

    fn queue_err(queue: &mut QueueCore, version: u64, ops: &[QueueOp]) -> QueueEditError {
        let before = queue.entries.clone();
        let err = queue.edit(version, ops).unwrap_err();
        assert_eq!(queue.entries, before, "a failed edit changes nothing");
        err
    }

    #[test]
    fn queue_preview_joins_text_blocks_within_the_label_limit() {
        let mut content = text("fix");
        content.push(PromptBlock::FileRef(FileRef {
            path: WirePath("a.rs".into()),
        }));
        content.extend(text(&"x".repeat(2_000)));
        let preview = preview(&content);
        assert!(preview.starts_with("fix x"));
        assert_eq!(preview.chars().count(), V1.max_label_chars as usize);
    }
}
