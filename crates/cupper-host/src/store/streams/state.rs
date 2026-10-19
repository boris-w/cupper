//! One stream in memory (design 5.2.3): its head, its view and its
//! subscribers, behind the stream lock. Under the lock only in-memory work
//! happens: no I/O, no await.
//!
//! The lock is what keeps SP-4a's invariant: the last step of an append
//! (apply, advance the head, fan out) and a subscription (read the head and
//! the view, register) exclude each other, so a subscriber gets every event
//! after its start exactly once, in order (PROP-stream-seq).

use std::fmt;
use std::sync::Arc;

use cupper_protocol::envelope::Event;
use cupper_protocol::event::EventBody;
use cupper_protocol::sync::{
    LogExtent, Resume, ResyncReason, StreamId, SubscribeMode, subscribe_mode,
};
use cupper_protocol::view::ViewError;

use super::sync::{Mutex, MutexGuard, lock};
use super::view::StreamView;

/// An appended event, serialized once for the log and every subscriber.
#[derive(Debug)]
pub struct Published {
    pub event: Event<EventBody>,
    /// `event` as JSON: the `events.json` column and the wire form.
    pub json: Box<str>,
}

/// What a [`Sink`] did with a live event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Push {
    Queued,
    /// Over the connection's budget (`limits.max_queue_bytes`); nothing was
    /// queued. The stream drops the subscriber and calls [`Sink::reset`].
    Full,
    /// The connection is gone; the stream drops the subscriber.
    Closed,
}

/// Where a subscriber's live events go: a connection's send queue (T033).
/// Both methods run under the stream lock, so they queue without waiting and
/// never call back into a stream.
pub trait Sink: Send + Sync + fmt::Debug {
    fn push(&self, stream: &StreamId, event: &Arc<Published>) -> Push;

    /// The stream dropped this subscriber after [`Push::Full`]: queue
    /// `stream/reset {reason: slow_consumer}` after what is queued already.
    fn reset(&self, stream: &StreamId);
}

/// How a subscription starts, decided under the stream lock (design 5.3.3).
#[derive(Debug)]
pub struct Start {
    pub mode: SubscribeMode,
    pub reason: Option<ResyncReason>,
    /// `replay`: the cursor's `after_seq`; the events `at_seq + 1..=head_seq`
    /// are committed and in the log. `snapshot`: the seq `ops` stand at.
    pub at_seq: u64,
    pub head_seq: u64,
    pub epoch: u64,
    /// `snapshot`: the view as ops. Empty for `replay`.
    pub ops: Vec<EventBody>,
}

/// A batch the writer thread has committed (step 2), for step 3.
#[derive(Debug)]
pub(super) struct Committed {
    pub events: Vec<Arc<Published>>,
    /// The view after the batch, when step 1 built it for a checkpoint.
    pub view: Option<StreamView>,
    /// The events up to this seq left the log.
    pub pruned: Option<u64>,
}

#[derive(Debug)]
pub(super) struct Stream {
    pub id: StreamId,
    state: Mutex<State>,
}

/// What the stream lock guards.
#[derive(Debug)]
pub(super) struct State {
    head: u64,
    /// The oldest seq in the log; `head + 1` while it keeps none.
    first_seq: u64,
    /// Where the checkpoint stands; 0 without one.
    checkpoint_at: u64,
    view: StreamView,
    subscribers: Vec<(u64, Arc<dyn Sink>)>,
    next_subscriber: u64,
    has_writer: bool,
}

impl State {
    pub fn new(head: u64, first_seq: u64, checkpoint_at: u64, view: StreamView) -> Self {
        Self {
            head,
            first_seq,
            checkpoint_at,
            view,
            subscribers: Vec::new(),
            next_subscriber: 0,
            has_writer: false,
        }
    }
}

impl Stream {
    pub fn new(id: StreamId, state: State) -> Self {
        Self {
            id,
            state: Mutex::new(state),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// Takes the one writer's place: the next seq and where the checkpoint
    /// stands, or `None` while another writer has it.
    pub fn attach_writer(&self) -> Option<(u64, u64)> {
        let mut state = self.lock();
        if state.has_writer {
            return None;
        }
        state.has_writer = true;
        Some((state.head + 1, state.checkpoint_at))
    }

    pub fn detach_writer(&self) {
        self.lock().has_writer = false;
    }

    pub fn read<T>(&self, read: impl FnOnce(&StreamView, u64) -> T) -> T {
        let state = self.lock();
        read(&state.view, state.head)
    }

    /// Registers `sink`, after `start` has queued how it starts: `start` runs
    /// under the stream lock, before any live event can reach `sink`.
    pub fn subscribe(
        &self,
        sink: Arc<dyn Sink>,
        resume: Option<Resume>,
        epoch: u64,
        start: impl FnOnce(Start),
    ) -> u64 {
        let mut state = self.lock();
        let log = LogExtent {
            epoch,
            head_seq: state.head,
            first_seq: state.first_seq,
        };
        let (mode, reason) = subscribe_mode(resume, &log);
        let (at_seq, ops) = match resume.filter(|_| mode == SubscribeMode::Replay) {
            Some(cursor) => (cursor.after_seq, Vec::new()),
            None => (state.head, state.view.snapshot_ops()),
        };
        start(Start {
            mode,
            reason,
            at_seq,
            head_seq: state.head,
            epoch,
            ops,
        });
        let id = state.next_subscriber;
        state.next_subscriber += 1;
        state.subscribers.push((id, sink));
        id
    }

    pub fn unsubscribe(&self, subscriber: u64) {
        self.lock().subscribers.retain(|(id, _)| *id != subscriber);
    }

    /// Step 3 of an append: the view moves past the batch, the head advances
    /// and every subscriber gets the events. Returns the events the view
    /// rejected.
    pub fn publish(&self, batch: Committed) -> Vec<(u64, ViewError)> {
        let mut state = self.lock();
        let state = &mut *state;
        let mut rejected = Vec::new();
        let (Some(first), Some(last)) = (batch.events.first(), batch.events.last()) else {
            return rejected;
        };
        assert_eq!(first.event.seq, state.head + 1, "batches publish in order");
        match batch.view {
            Some(view) => (state.view, state.checkpoint_at) = (view, last.event.seq),
            None => {
                for published in &batch.events {
                    if let Err(err) = state.view.fold(&published.event.body, &mut |_, _| {}) {
                        rejected.push((published.event.seq, err));
                    }
                }
            }
        }
        state.head = last.event.seq;
        if let Some(cut) = batch.pruned {
            state.first_seq = state.first_seq.max(cut + 1);
        }
        for event in &batch.events {
            state
                .subscribers
                .retain(|(_, sink)| match sink.push(&self.id, event) {
                    Push::Queued => true,
                    Push::Full => {
                        sink.reset(&self.id);
                        false
                    }
                    Push::Closed => false,
                });
        }
        rejected
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Mutex;

    use cupper_protocol::annotations::TimestampMs;
    use cupper_protocol::event::Strictness;
    use serde_json::json;

    use super::*;

    /// `project.upserted` of `p_<n>`, or `project.removed` of `p_<n>` with `removed`.
    pub fn project(n: u64, removed: bool) -> EventBody {
        let raw = match removed {
            false => json!({"type": "project.upserted", "data": {"project_id": format!("p_{n}"),
                "name": "p", "path": "/w", "is_git": false, "added_at": n, "position": n,
                "trust_state": "trusted"}}),
            true => json!({"type": "project.removed", "data": {"project_id": format!("p_{n}")}}),
        };
        EventBody::decode(&raw, Strictness::Strict).unwrap().value
    }

    pub fn batch(seqs: impl IntoIterator<Item = u64>) -> Committed {
        let events = seqs.into_iter().map(|seq| {
            let body = project(seq, false);
            let event = Event {
                seq,
                from: None,
                v: 1,
                ts: TimestampMs(0),
                cause: None,
                fallback: None,
                body,
            };
            let json = serde_json::to_string(&event).unwrap().into_boxed_str();
            Arc::new(Published { event, json })
        });
        Committed {
            events: events.collect(),
            view: None,
            pruned: None,
        }
    }

    /// Collects the seqs pushed to it; full after `budget` of them.
    #[derive(Debug, Default)]
    pub struct Collect {
        pub budget: usize,
        pub seqs: Mutex<Vec<u64>>,
        pub resets: Mutex<usize>,
    }

    impl Sink for Collect {
        fn push(&self, _: &StreamId, event: &Arc<Published>) -> Push {
            let mut seqs = self.seqs.lock().unwrap();
            if seqs.len() == self.budget {
                return Push::Full;
            }
            seqs.push(event.event.seq);
            Push::Queued
        }

        fn reset(&self, _: &StreamId) {
            *self.resets.lock().unwrap() += 1;
        }
    }

    fn sink(budget: usize) -> Arc<Collect> {
        Arc::new(Collect {
            budget,
            ..Collect::default()
        })
    }

    fn index(head: u64, first_seq: u64) -> Stream {
        let view = StreamView::empty(&StreamId::Index);
        Stream::new(StreamId::Index, State::new(head, first_seq, 0, view))
    }

    fn start(stream: &Stream, sink: &Arc<Collect>, resume: Option<(u64, u64)>) -> (Start, u64) {
        let resume = resume.map(|(epoch, after_seq)| Resume { epoch, after_seq });
        let mut started = None;
        let id = stream.subscribe(sink.clone(), resume, 7, |s| started = Some(s));
        (started.unwrap(), id)
    }

    #[test]
    fn store_streams_subscribers_start_at_the_head_and_get_what_follows() {
        let stream = index(0, 1);
        assert!(stream.publish(batch(1..=3)).is_empty());
        let (replaying, snapshotting) = (sink(9), sink(9));
        let (replay, replay_id) = start(&stream, &replaying, Some((7, 1)));
        assert_eq!(
            (replay.mode, replay.at_seq, replay.head_seq),
            (SubscribeMode::Replay, 1, 3)
        );
        assert!(replay.ops.is_empty());
        let (snapshot, _) = start(&stream, &snapshotting, Some((6, 1)));
        assert_eq!(
            (snapshot.mode, snapshot.reason),
            (SubscribeMode::Snapshot, Some(ResyncReason::EpochChanged))
        );
        assert_eq!((snapshot.at_seq, snapshot.ops.len()), (3, 3));

        // A body the view rejects still goes out.
        let mut removal = batch([4]);
        let mut event = removal.events.pop().unwrap().event.clone();
        event.body = project(9, true);
        let json = serde_json::to_string(&event).unwrap().into_boxed_str();
        removal.events.push(Arc::new(Published { event, json }));
        assert_eq!(
            stream.publish(removal),
            [(4, ViewError::Missing("p_9".into()))]
        );
        stream.unsubscribe(replay_id);
        stream.publish(batch([5]));
        assert_eq!(*replaying.seqs.lock().unwrap(), [4]);
        assert_eq!(*snapshotting.seqs.lock().unwrap(), [4, 5]);
        assert_eq!(
            stream.read(|view, head| (view.snapshot_ops().len(), head)),
            (4, 5)
        );
    }

    #[test]
    fn store_streams_slow_subscriber_is_reset_once_and_dropped() {
        let stream = index(0, 1);
        let (slow, fine) = (sink(1), sink(9));
        drop(start(&stream, &slow, None));
        drop(start(&stream, &fine, None));
        stream.publish(batch([1, 2]));
        stream.publish(batch([3]));
        assert_eq!(
            (
                slow.seqs.lock().unwrap().clone(),
                *slow.resets.lock().unwrap()
            ),
            (vec![1], 1)
        );
        assert_eq!(*fine.seqs.lock().unwrap(), [1, 2, 3]);
    }

    #[test]
    fn store_streams_one_writer_at_a_time_and_pruned_cursors_expire() {
        let stream = index(4, 5);
        assert_eq!(stream.attach_writer(), Some((5, 0)));
        assert_eq!(stream.attach_writer(), None);
        stream.detach_writer();
        assert_eq!(stream.attach_writer(), Some((5, 0)));

        let mut checkpointed = batch([5, 6]);
        checkpointed.view = Some(StreamView::empty(&StreamId::Index));
        checkpointed.pruned = Some(5);
        stream.publish(checkpointed);
        stream.detach_writer();
        assert_eq!(stream.attach_writer(), Some((7, 6)), "the checkpoint moved");
        let (expired, _) = start(&stream, &sink(9), Some((7, 4)));
        assert_eq!(expired.reason, Some(ResyncReason::CursorExpired));
        assert!(expired.ops.is_empty(), "the view came from the batch");
        assert_eq!(
            start(&stream, &sink(9), Some((7, 5))).0.mode,
            SubscribeMode::Replay
        );
    }
}
