//! The connection's writer (design 5.2.4): it takes what the outbox queued
//! and writes it, in order.
//!
//! - With a coalescing window (`coalesce_ms`), having taken a live event it
//!   waits that long on the clock for more before it sends.
//! - Live events of one feed in a row go out in one `stream/events`, as many
//!   to a frame as fit. When the client agreed to `stream.coalesce`,
//!   contiguous deltas merge into range events first (design 5.3.3).
//! - A snapshot goes out as its pages. A replay is read from the log a page
//!   at a time and sent like live events.
//! - A replay that finds events missing (the log was pruned after the
//!   subscription chose to replay) or cannot read them ends its feed with
//!   `stream/reset {reason: rebuilt}` (design 5.3.11, rulings before T033):
//!   the client resubscribes with its cursor and gets a `cursor_expired`
//!   snapshot. So does an event too large for a frame of its own, which the
//!   host never makes.

use std::borrow::Cow;
use std::io;
use std::sync::Arc;

use cupper_protocol::coalesce::try_merge;
use cupper_protocol::envelope::Event;
use cupper_protocol::event::EventBody;
use cupper_protocol::methods::stream::ResetReason;
use cupper_protocol::sync::StreamId;
use cupper_protocol::text::MAX_MERGED_ESCAPED;
use tokio::io::AsyncWriteExt;
use tokio::net::unix::OwnedWriteHalf;

use super::outbox::{Item, Outbox};
use super::wire;
use crate::clock::Clock;
use crate::quiescence::Participant;
use crate::store::Readers;
use crate::store::log;
use crate::store::streams::Published;

/// How many events a replay reads from the log at a time.
const REPLAY_PAGE: u64 = 256;

pub(super) async fn write_loop(
    mut writer: OwnedWriteHalf,
    outbox: Arc<Outbox>,
    clock: Arc<dyn Clock>,
    participant: Participant,
) {
    while let Some(mut items) = participant.idle(outbox.drain()).await {
        let coalesce = outbox.coalesce();
        let live = items.iter().any(|item| matches!(item, Item::Live { .. }));
        if live && !coalesce.window.is_zero() {
            let window = clock.timer_at(clock.now_mono() + coalesce.window);
            participant.idle(window).await;
            items.extend(outbox.take());
        }
        let mut send = Send {
            writer: &mut writer,
            outbox: &outbox,
            ranges: coalesce.ranges,
            run: Vec::new(),
            run_of: None,
            ended: Vec::new(),
        };
        if send.items(items).await.is_err() {
            outbox.close();
            return;
        }
    }
}

/// One batch on its way out.
struct Send<'a> {
    writer: &'a mut OwnedWriteHalf,
    outbox: &'a Outbox,
    ranges: bool,
    /// Events of one feed in a row, not written yet.
    run: Vec<Arc<Published>>,
    run_of: Option<(StreamId, u64)>,
    /// Feeds this batch ended: what else it holds of them is not sent.
    ended: Vec<u64>,
}

impl Send<'_> {
    async fn items(&mut self, items: Vec<Item>) -> io::Result<()> {
        for item in items {
            if item.feed().is_some_and(|feed| self.ended.contains(&feed)) {
                continue;
            }
            if !matches!(item, Item::Live { .. }) {
                self.flush().await?;
            }
            match item {
                Item::Frame(frame) => self.writer.write_all(&frame).await?,
                Item::Live {
                    stream,
                    feed,
                    event,
                } => {
                    if self.run_of.as_ref().is_none_or(|(_, of)| *of != feed) {
                        self.flush().await?;
                        self.run_of = Some((stream, feed));
                    }
                    self.run.push(event);
                }
                Item::Snapshot {
                    stream,
                    at_seq,
                    epoch,
                    ops,
                    ..
                } => {
                    for frame in wire::snapshot(&stream, at_seq, epoch, &ops) {
                        self.writer.write_all(&frame).await?;
                    }
                }
                Item::Replay {
                    stream,
                    feed,
                    after,
                    up_to,
                    readers,
                } => self.replay(stream, feed, after, up_to, &readers).await?,
            }
        }
        self.flush().await
    }

    async fn flush(&mut self) -> io::Result<()> {
        let Some((stream, feed)) = self.run_of.take() else {
            return Ok(());
        };
        let run = std::mem::take(&mut self.run);
        let (frames, whole) = wire::events(&stream, &merged(&run, self.ranges));
        for frame in frames {
            self.writer.write_all(&frame).await?;
        }
        if !whole {
            self.end(&stream, feed);
        }
        Ok(())
    }

    async fn replay(
        &mut self,
        stream: StreamId,
        feed: u64,
        mut after: u64,
        up_to: u64,
        readers: &Readers,
    ) -> io::Result<()> {
        while after < up_to && !self.ended.contains(&feed) {
            let end = up_to.min(after + REPLAY_PAGE);
            let of = stream.clone();
            let read = readers.read(move |conn| {
                let mut events = Vec::new();
                let read = log::read_events(conn, &of, after, end, |event| events.push(event));
                Ok(read.map(|()| events))
            });
            match read.await {
                // Seqs are unique: as many events as seqs means none is missing.
                Ok(Ok(events)) if events.len() as u64 == end - after => {
                    self.run = events.into_iter().map(published).collect();
                    self.run_of = Some((stream.clone(), feed));
                    self.flush().await?;
                }
                _ => self.end(&stream, feed),
            }
            after = end;
        }
        Ok(())
    }

    fn end(&mut self, stream: &StreamId, feed: u64) {
        self.ended.push(feed);
        self.outbox
            .end_feed(stream, feed, Some(ResetReason::Rebuilt));
    }
}

fn published(event: Event<EventBody>) -> Arc<Published> {
    let json = serde_json::to_string(&event).expect("an event serializes");
    let json = json.into_boxed_str();
    Arc::new(Published { event, json })
}

/// The JSON of the events of `run`; with `ranges`, contiguous deltas merged
/// into range events.
fn merged(run: &[Arc<Published>], ranges: bool) -> Vec<Cow<'_, str>> {
    let mut out = Vec::with_capacity(run.len());
    let mut at = 0;
    while at < run.len() {
        let (mut merged, mut next) = (None, at + 1);
        while ranges && next < run.len() {
            let prev = merged.as_ref().unwrap_or(&run[at].event);
            let Some(event) = try_merge(prev, &run[next].event, MAX_MERGED_ESCAPED) else {
                break;
            };
            merged = Some(event);
            next += 1;
        }
        out.push(match merged {
            Some(event) => Cow::Owned(serde_json::to_string(&event).expect("an event serializes")),
            None => Cow::Borrowed(&*run[at].json),
        });
        at = next;
    }
    out
}
