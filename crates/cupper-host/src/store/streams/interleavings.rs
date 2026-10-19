//! PROP-stream-seq (design 5.9.8): shuttle runs a writer's appends and two
//! subscriptions of one stream through every interleaving of their locks.
//! Built with `--cfg shuttle` only, which `cargo xtask ci` does.

use std::sync::Arc;

use cupper_protocol::sync::{Resume, StreamId, SubscribeMode};
use shuttle::thread;

use super::state::tests::{Collect, batch};
use super::state::{State, Stream};
use super::view::StreamView;

/// Whatever the order, a subscriber sees every seq after its start once, in
/// order: replayed up to the head it started at, then live. The writer
/// commits a batch before it publishes it (step 2 before step 3), so the
/// log holds every seq up to the head.
#[test]
fn prop_stream_seq_subscribers_see_every_seq_after_their_start_once() {
    shuttle::check_dfs(
        || {
            let view = StreamView::empty(&StreamId::Index);
            let stream = Arc::new(Stream::new(StreamId::Index, State::new(0, 1, 0, view)));
            let writer = thread::spawn({
                let stream = stream.clone();
                move || {
                    stream.publish(batch(1..=2));
                    stream.publish(batch([3]));
                }
            });
            let subscribers = [None, Some(1)].map(|after_seq| {
                let stream = stream.clone();
                thread::spawn(move || {
                    let sink = Arc::new(Collect {
                        budget: 9,
                        ..Collect::default()
                    });
                    let resume = after_seq.map(|after_seq| Resume {
                        epoch: 1,
                        after_seq,
                    });
                    let mut start = None;
                    stream.subscribe(sink.clone(), resume, 1, |s| start = Some(s));
                    (start.unwrap(), sink)
                })
            });
            writer.join().unwrap();
            for subscriber in subscribers {
                let (start, sink) = subscriber.join().unwrap();
                let mut seen: Vec<u64> = match start.mode {
                    SubscribeMode::Replay => (start.at_seq + 1..=start.head_seq).collect(),
                    // One project per seq: the snapshot stands at `at_seq`.
                    _ => {
                        assert_eq!(start.ops.len() as u64, start.at_seq);
                        Vec::new()
                    }
                };
                seen.extend(sink.seqs.lock().unwrap().iter());
                assert_eq!(seen, (start.at_seq + 1..=3).collect::<Vec<_>>());
            }
        },
        None,
    );
}
