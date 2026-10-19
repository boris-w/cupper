//! The client side of stream sync (design 5.3.3, T014's rulings in 5.3.11):
//! a [`Mirror`] keeps one stream's view up to date from what the host sends.
//!
//! The mirror is a pure core; [`subscribe`] and [`fold`] connect it to a
//! [`Client`]. Its rules:
//!
//! - Only a `replay` that fits the cursor keeps the view and the cursor. Any
//!   other subscription, a snapshot whatever the reason or a mode this
//!   version does not know, drops both: after `epoch_changed` seqs may be
//!   reused, and nothing old is left to clash with them.
//! - Snapshot pages must match the subscription's `at_seq` and `epoch` and
//!   come in order.
//! - Events must be contiguous ([`StreamCursor`]); after a gap the mirror
//!   subscribes again with its cursor. After a [`ViewError`](cupper_protocol::view::ViewError),
//!   or an event without a readable envelope, it starts over without one.
//! - `stream/reset` keeps the cursor.
//! - Until the result of a subscription arrives, whatever else comes for the
//!   stream belongs to an older subscription and is ignored.

use cupper_protocol::error::RpcError;
use cupper_protocol::event::Strictness;
use cupper_protocol::ids::SessionId;
use cupper_protocol::methods::stream::{
    StreamEventsParams, StreamSnapshotParams, StreamSubscribeParams, StreamSubscribeResult,
};
use cupper_protocol::sync::{Resume, StreamCursor, StreamId, SubscribeMode};
use cupper_protocol::view::{IndexView, SessionView, View};

use crate::client::{Client, ClientError, Incoming};

/// One stream as the client has it.
#[derive(Debug, Clone)]
pub struct Mirror<V> {
    stream: StreamId,
    /// The latest complete view; empty once a snapshot or a restart dropped it.
    view: V,
    /// Where `view` stands.
    cursor: Option<Cursor>,
    state: State<V>,
}

#[derive(Debug, Clone, Copy)]
struct Cursor {
    epoch: u64,
    seq: StreamCursor,
}

#[derive(Debug, Clone)]
enum State<V> {
    /// Not subscribed, or waiting for the result of `stream/subscribe`.
    Waiting,
    /// Snapshot pages at `at_seq` of `epoch`; `page` is the next one.
    Loading {
        at_seq: u64,
        epoch: u64,
        page: u32,
        view: V,
    },
    /// Events follow the cursor.
    Live,
}

impl Mirror<IndexView> {
    pub fn index() -> Self {
        Self::new(StreamId::Index)
    }
}

impl Mirror<SessionView> {
    pub fn session(id: SessionId) -> Self {
        Self::new(StreamId::Session(id))
    }
}

impl<V: View> Mirror<V> {
    fn new(stream: StreamId) -> Self {
        Self {
            stream,
            view: V::default(),
            cursor: None,
            state: State::Waiting,
        }
    }

    pub fn stream(&self) -> &StreamId {
        &self.stream
    }

    /// The latest complete view, as of [`Mirror::resume`].
    pub fn view(&self) -> &V {
        &self.view
    }

    /// Whether the view is complete and events follow it.
    pub fn is_live(&self) -> bool {
        matches!(self.state, State::Live)
    }

    /// The cursor the next subscription resumes from.
    pub fn resume(&self) -> Option<Resume> {
        self.cursor.map(|c| Resume {
            epoch: c.epoch,
            after_seq: c.seq.last_seq(),
        })
    }

    /// Parameters of `stream/subscribe`, with the cursor when there is one.
    /// Until its result, data for the stream is ignored.
    pub fn subscribe(&mut self) -> StreamSubscribeParams {
        self.state = State::Waiting;
        let resume = self.resume();
        StreamSubscribeParams {
            stream: self.stream.clone(),
            after_seq: resume.map(|r| r.after_seq),
            epoch: resume.map(|r| r.epoch),
        }
    }

    /// Folds one message from the host. `Ok(Some(params))`: subscribe again
    /// with `params`. A failed subscription comes back as its error.
    pub fn handle(
        &mut self,
        incoming: &Incoming,
    ) -> Result<Option<StreamSubscribeParams>, RpcError> {
        Ok(match incoming {
            Incoming::Subscribed { stream, result } if *stream == self.stream => {
                self.subscribed(result.as_ref().map_err(RpcError::clone)?)
            }
            Incoming::Snapshot(page) if page.stream == self.stream => self.snapshot(page),
            Incoming::Events(events) if events.stream == self.stream => self.events(events),
            Incoming::Reset(reset) if reset.stream == self.stream => match self.state {
                State::Waiting => None,
                _ => Some(self.subscribe()),
            },
            _ => None,
        })
    }

    fn subscribed(&mut self, result: &StreamSubscribeResult) -> Option<StreamSubscribeParams> {
        if result.mode == SubscribeMode::Replay {
            let at = Resume {
                epoch: result.epoch,
                after_seq: result.at_seq,
            };
            if self.resume() != Some(at) {
                return self.restart();
            }
            self.state = State::Live;
            return None;
        }
        self.view = V::default();
        self.cursor = None;
        self.state = State::Loading {
            at_seq: result.at_seq,
            epoch: result.epoch,
            page: 0,
            view: V::default(),
        };
        None
    }

    fn snapshot(&mut self, page: &StreamSnapshotParams) -> Option<StreamSubscribeParams> {
        let State::Loading {
            at_seq,
            epoch,
            page: next,
            view,
        } = &mut self.state
        else {
            return None;
        };
        if (page.at_seq, page.epoch, page.page) != (*at_seq, *epoch, *next) {
            return self.restart();
        }
        for op in &page.ops {
            // An op that does not decode at all changes nothing, as an
            // unknown type would not.
            let Ok(op) = op.decode(Strictness::Tolerant) else {
                continue;
            };
            if view.apply(&op.value).is_err() {
                return self.restart();
            }
        }
        *next += 1;
        if page.last {
            let cursor = Cursor {
                epoch: *epoch,
                seq: StreamCursor::at(*at_seq),
            };
            if let State::Loading { view, .. } = std::mem::replace(&mut self.state, State::Live) {
                self.view = view;
            }
            self.cursor = Some(cursor);
        }
        None
    }

    fn events(&mut self, events: &StreamEventsParams) -> Option<StreamSubscribeParams> {
        match self.state {
            State::Waiting => return None,
            // Events only follow the last page.
            State::Loading { .. } => return self.restart(),
            State::Live => {}
        }
        let Some(cursor) = self.cursor.as_mut() else {
            return self.restart();
        };
        for raw in &events.events {
            let Ok(event) = raw.decode(Strictness::Tolerant) else {
                return self.restart();
            };
            if cursor.seq.accept(&event.value).is_err() {
                return Some(self.subscribe());
            }
            if self.view.apply(&event.value.body).is_err() {
                return self.restart();
            }
        }
        None
    }

    /// Drops the view and the cursor, and subscribes for a snapshot.
    fn restart(&mut self) -> Option<StreamSubscribeParams> {
        self.view = V::default();
        self.cursor = None;
        Some(self.subscribe())
    }
}

/// Subscribes `mirror` over `client`.
pub async fn subscribe<V: View>(
    client: &Client,
    mirror: &mut Mirror<V>,
) -> Result<(), ClientError> {
    client.subscribe(mirror.subscribe()).await
}

/// Folds `incoming` into `mirror`, subscribing again over `client` when the
/// mirror asks to.
pub async fn fold<V: View>(
    client: &Client,
    mirror: &mut Mirror<V>,
    incoming: &Incoming,
) -> Result<(), ClientError> {
    if let Some(params) = mirror.handle(incoming)? {
        client.subscribe(params).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use cupper_protocol::view::ItemView;
    use serde_json::{Value, json};

    use super::*;

    fn stream() -> StreamId {
        StreamId::Session("s_1".parse().unwrap())
    }

    fn turn(n: u32) -> Value {
        json!({"type": "turn.started", "data": {"turn_id": format!("t_{n}"), "turn_no": n,
            "origin": "user", "started_at": n}})
    }

    fn item(id: &str) -> Value {
        json!({"type": "item.started", "data": {"item_id": id, "turn_id": "t_1", "started_at": 1,
            "kind": "agent_message"}})
    }

    fn delta(id: &str, text: &str) -> Value {
        json!({"type": "item.delta", "data": {"item_id": id, "field": "text", "text": text}})
    }

    /// An event at `seq` with `body`; `from` makes it a range event.
    fn event(seq: u64, from: Option<u64>, body: Value) -> Value {
        let mut event = json!({"seq": seq, "v": 1, "ts": 1});
        if let Some(from) = from {
            event["from"] = from.into();
        }
        event
            .as_object_mut()
            .unwrap()
            .extend(body.as_object().unwrap().clone());
        event
    }

    fn events(events: Vec<Value>) -> Incoming {
        Incoming::Events(
            serde_json::from_value(json!({"stream": stream(), "events": events})).unwrap(),
        )
    }

    fn page(at_seq: u64, epoch: u64, page: u32, last: bool, ops: Vec<Value>) -> Incoming {
        let page = json!({"stream": stream(), "at_seq": at_seq, "epoch": epoch, "page": page,
            "last": last, "ops": ops});
        Incoming::Snapshot(serde_json::from_value(page).unwrap())
    }

    fn subscribed(mode: &str, reason: Option<&str>, at_seq: u64, epoch: u64) -> Incoming {
        let mut result = json!({"stream": stream(), "mode": mode, "at_seq": at_seq,
            "head_seq": at_seq, "epoch": epoch});
        if let Some(reason) = reason {
            result["reason"] = reason.into();
        }
        let result = Ok(serde_json::from_value(result).unwrap());
        Incoming::Subscribed {
            stream: stream(),
            result,
        }
    }

    fn reset() -> Incoming {
        Incoming::Reset(
            serde_json::from_value(json!({"stream": stream(), "reason": "slow_consumer"})).unwrap(),
        )
    }

    fn text(mirror: &Mirror<SessionView>) -> Option<&str> {
        let item = mirror.view().items.get(&"i_1".parse().unwrap());
        item.map(|ItemView { text, .. }| text.as_str())
    }

    fn cursor(epoch: u64, after_seq: u64) -> Option<Resume> {
        Some(Resume { epoch, after_seq })
    }

    /// The parameters the mirror asks to subscribe with, as `(after_seq, epoch)`.
    fn again(asked: Option<StreamSubscribeParams>) -> (Option<u64>, Option<u64>) {
        let params = asked.expect("the mirror asks to subscribe again");
        assert_eq!(params.stream, stream());
        (params.after_seq, params.epoch)
    }

    fn handle(
        mirror: &mut Mirror<SessionView>,
        incoming: Incoming,
    ) -> Option<StreamSubscribeParams> {
        mirror.handle(&incoming).unwrap()
    }

    /// Live at seq 3 of epoch 1, with item `i_1` reading "a".
    fn live() -> Mirror<SessionView> {
        let mut mirror = Mirror::session("s_1".parse().unwrap());
        assert_eq!(again(Some(mirror.subscribe())), (None, None));
        assert_eq!(
            handle(&mut mirror, subscribed("snapshot", None, 2, 1)),
            None
        );
        assert_eq!(
            handle(&mut mirror, page(2, 1, 0, true, vec![turn(1), item("i_1")])),
            None
        );
        assert_eq!(
            handle(&mut mirror, events(vec![event(3, None, delta("i_1", "a"))])),
            None
        );
        assert!(mirror.is_live());
        assert_eq!((text(&mirror), mirror.resume()), (Some("a"), cursor(1, 3)));
        mirror
    }

    #[test]
    fn unit_mirror_snapshot_pages_assemble_the_view() {
        let ops = [
            vec![turn(1), item("i_1")],
            vec![delta("i_1", "a"), delta("i_1", "b")],
        ];
        let mut mirror = Mirror::session("s_1".parse().unwrap());
        mirror.subscribe();
        handle(&mut mirror, subscribed("snapshot", None, 5, 1));
        assert_eq!(
            handle(&mut mirror, page(5, 1, 0, false, ops[0].clone())),
            None
        );
        assert!(!mirror.is_live());
        assert_eq!(
            (text(&mirror), mirror.resume()),
            (None, None),
            "nothing before the last page"
        );
        assert_eq!(
            handle(&mut mirror, page(5, 1, 1, true, ops[1].clone())),
            None
        );
        assert!(mirror.is_live());
        assert_eq!(mirror.resume(), cursor(1, 5));
        let mut direct = SessionView::default();
        for op in ops.concat() {
            direct.apply(&serde_json::from_value(op).unwrap()).unwrap();
        }
        assert_eq!(*mirror.view(), direct);

        // A page of another `at_seq`, another epoch, or out of order starts over.
        for wrong in [
            page(6, 1, 0, true, vec![]),
            page(5, 2, 0, true, vec![]),
            page(5, 1, 1, true, vec![]),
        ] {
            let mut mirror = Mirror::session("s_1".parse().unwrap());
            mirror.subscribe();
            handle(&mut mirror, subscribed("snapshot", None, 5, 1));
            assert_eq!(again(handle(&mut mirror, wrong)), (None, None));
            assert!(!mirror.is_live());
        }
    }

    #[test]
    fn unit_mirror_any_snapshot_drops_the_view_and_cursor() {
        let results = [
            subscribed("snapshot", None, 9, 1),
            subscribed("snapshot", Some("cursor_expired"), 9, 1),
            subscribed("snapshot", Some("cursor_ahead"), 9, 1),
            subscribed("rebuild", Some("whatever"), 9, 1),
        ];
        for result in results {
            let mut mirror = live();
            assert_eq!(again(Some(mirror.subscribe())), (Some(3), Some(1)));
            assert_eq!(handle(&mut mirror, result.clone()), None, "{result:?}");
            assert_eq!((text(&mirror), mirror.resume()), (None, None), "{result:?}");
            assert_eq!(handle(&mut mirror, page(9, 1, 0, true, vec![])), None);
            assert!(mirror.is_live());
        }
        // A replay that does not fit the cursor starts over without one.
        for (at_seq, epoch) in [(2, 1), (3, 2)] {
            let mut mirror = live();
            mirror.subscribe();
            let replay = subscribed("replay", None, at_seq, epoch);
            assert_eq!(again(handle(&mut mirror, replay)), (None, None));
            assert_eq!(text(&mirror), None);
        }
    }

    #[test]
    fn unit_mirror_gap_resubscribes_with_the_cursor() {
        let mut mirror = live();
        let contiguous = vec![
            event(4, None, delta("i_1", "b")),
            event(6, Some(5), delta("i_1", "cd")),
        ];
        assert_eq!(handle(&mut mirror, events(contiguous)), None);
        assert_eq!(
            (text(&mirror), mirror.resume()),
            (Some("abcd"), cursor(1, 6))
        );
        for gap in [
            event(8, None, delta("i_1", "x")),
            event(6, None, delta("i_1", "x")),
        ] {
            let mut mirror = mirror.clone();
            assert_eq!(
                again(handle(&mut mirror, events(vec![gap]))),
                (Some(6), Some(1))
            );
            assert_eq!(text(&mirror), Some("abcd"), "the view stays");
            assert_eq!(handle(&mut mirror, subscribed("replay", None, 6, 1)), None);
            assert_eq!(
                handle(&mut mirror, events(vec![event(7, None, delta("i_1", "e"))])),
                None
            );
            assert_eq!(
                (text(&mirror), mirror.resume()),
                (Some("abcde"), cursor(1, 7))
            );
        }
    }

    #[test]
    fn unit_mirror_view_error_resubscribes_without_the_cursor() {
        let mut no_seq = event(4, None, delta("i_1", "b"));
        no_seq.as_object_mut().unwrap().remove("seq");
        let cases = [
            events(vec![event(4, None, delta("i_9", "b"))]),
            events(vec![no_seq]),
        ];
        for case in cases {
            let mut mirror = live();
            assert_eq!(
                again(handle(&mut mirror, case.clone())),
                (None, None),
                "{case:?}"
            );
            assert_eq!((text(&mirror), mirror.is_live()), (None, false));
        }
        // Inside a snapshot: a delta before its item, and events before the last page.
        let broken = page(5, 1, 0, true, vec![turn(1), delta("i_1", "a")]);
        for case in [broken, events(vec![event(6, None, turn(2))])] {
            let mut mirror = live();
            mirror.subscribe();
            handle(&mut mirror, subscribed("snapshot", None, 5, 1));
            assert_eq!(
                again(handle(&mut mirror, case.clone())),
                (None, None),
                "{case:?}"
            );
        }
    }

    #[test]
    fn unit_mirror_reset_keeps_the_cursor() {
        let mut mirror = live();
        assert_eq!(again(handle(&mut mirror, reset())), (Some(3), Some(1)));
        assert_eq!(text(&mirror), Some("a"));
        assert_eq!(handle(&mut mirror, reset()), None, "already waiting");
        assert_eq!(handle(&mut mirror, subscribed("replay", None, 3, 1)), None);
        assert!(mirror.is_live());
    }

    #[test]
    fn unit_mirror_epoch_change_resets_and_reuses_seqs() {
        let mut mirror = live();
        mirror.subscribe();
        let changed = subscribed("snapshot", Some("epoch_changed"), 1, 2);
        assert_eq!(handle(&mut mirror, changed), None);
        assert_eq!((text(&mirror), mirror.resume()), (None, None));
        assert_eq!(
            handle(&mut mirror, page(1, 2, 0, true, vec![turn(1)])),
            None
        );
        // Seqs 2 and 3 come again, now in epoch 2.
        let reused = vec![
            event(2, None, item("i_1")),
            event(3, None, delta("i_1", "z")),
        ];
        assert_eq!(handle(&mut mirror, events(reused)), None);
        assert_eq!((text(&mirror), mirror.resume()), (Some("z"), cursor(2, 3)));
    }

    #[test]
    fn unit_mirror_waiting_ignores_the_old_subscription() {
        let mut mirror = live();
        assert_eq!(again(Some(mirror.subscribe())), (Some(3), Some(1)));
        let old = [
            events(vec![event(4, None, delta("i_1", "b"))]),
            events(vec![event(9, None, delta("i_9", "b"))]),
            page(3, 1, 0, true, vec![]),
            reset(),
        ];
        for incoming in old {
            assert_eq!(handle(&mut mirror, incoming), None);
        }
        assert_eq!((text(&mirror), mirror.resume()), (Some("a"), cursor(1, 3)));

        // Another stream's messages are not this mirror's.
        let mut index = Mirror::index();
        index.subscribe();
        assert_eq!(
            index.handle(&subscribed("snapshot", None, 1, 1)).unwrap(),
            None
        );
        assert_eq!(index.handle(&page(1, 1, 0, true, vec![])).unwrap(), None);
        assert!(!index.is_live());

        // A failed subscription is the caller's to handle.
        let error = RpcError::new(-32000, "no such session");
        let failed = Incoming::Subscribed {
            stream: stream(),
            result: Err(error.clone()),
        };
        assert_eq!(mirror.handle(&failed), Err(error));
    }
}
