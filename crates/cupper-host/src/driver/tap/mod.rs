//! The wire tap (design 5.6.10): every line between an adapter instance and the
//! ACP SDK passes through it, in both directions. It numbers each line within
//! the instance and queues it as a raw frame after structured redaction
//! ([`redact`]). Extension messages are not its business: they all go through
//! the SDK (T003).
//!
//! The raw frame queue is bounded; when it is full the frame is dropped and
//! counted, so diagnostics never hold up the ACP read path (design 5.2.3).

mod redact;

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll, ready};

use agent_client_protocol::Lines;
use futures::{Sink, Stream};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader};
use tokio::sync::mpsc;

use super::transport::AcpTransport;
use crate::clock::Clock;

/// Raw frames waiting for the store's writer thread (design 5.2.3).
pub const RAW_QUEUE_CAPACITY: usize = 4096;

/// One line as `raw.db` keeps it (design 5.4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawFrame {
    pub instance: Arc<str>,
    /// Counts the instance's lines in both directions from 1.
    pub seq: u64,
    /// Wall-clock milliseconds.
    pub ts: u64,
    pub dir: Dir,
    /// The first `sessionId` in the line, for exporting one session's frames.
    pub agent_session_id: Option<String>,
    /// The line after structured redaction.
    pub frame: String,
}

/// Which way a line went (`raw_frames.dir`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Host to adapter.
    Send,
    /// Adapter to host.
    Recv,
    /// A line the adapter wrote that is not JSON; the SDK never sees it.
    RecvNonjson,
}

impl Dir {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Recv => "recv",
            Self::RecvNonjson => "recv_nonjson",
        }
    }
}

/// The sending side of the raw frame queue, shared by every instance's tap.
#[derive(Debug, Clone)]
pub struct RawSink {
    tx: mpsc::Sender<RawFrame>,
    dropped: Arc<AtomicU64>,
}

impl RawSink {
    /// A queue of [`RAW_QUEUE_CAPACITY`] frames and its reading side.
    pub fn bounded() -> (Self, mpsc::Receiver<RawFrame>) {
        let (tx, rx) = mpsc::channel(RAW_QUEUE_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        (Self { tx, dropped }, rx)
    }

    /// Frames lost because the queue was full or its reader had gone.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    fn push(&self, frame: RawFrame) {
        if self.tx.try_send(frame).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The tap of one adapter instance.
#[derive(Debug)]
pub struct Tap {
    instance: Arc<str>,
    clock: Arc<dyn Clock>,
    raw: RawSink,
    last_seq: AtomicU64,
}

impl Tap {
    pub fn new(instance: impl Into<Arc<str>>, clock: Arc<dyn Clock>, raw: RawSink) -> Self {
        Self {
            instance: instance.into(),
            clock,
            raw,
            last_seq: AtomicU64::new(0),
        }
    }

    /// The transport as the SDK's line component, with this tap on every line.
    pub fn wire<T: AcpTransport>(
        self,
        transport: T,
    ) -> Lines<
        impl Sink<String, Error = io::Error> + Send + 'static,
        impl Stream<Item = io::Result<String>> + Send + 'static,
    > {
        let tap = Arc::new(self);
        let (reader, writer) = transport.into_split();
        let outgoing = LineSink {
            writer,
            tap: tap.clone(),
            pending: Vec::new(),
            written: 0,
        };
        Lines::new(outgoing, incoming(reader, tap))
    }

    /// Records a line the adapter wrote; returns what the SDK gets, if
    /// anything. Lines that are not JSON are only recorded.
    fn inbound(&self, line: String) -> Option<String> {
        if !line.trim_start().starts_with(['{', '[']) {
            self.record(Dir::RecvNonjson, &line);
            return None;
        }
        let msg = (line.contains(redact::AUTH_STATUS_METHOD))
            .then(|| serde_json::from_str::<Value>(&line).ok())
            .flatten();
        match msg.as_ref().and_then(redact::account_fields) {
            Some(redacted) => self.record(Dir::Recv, &redacted.to_string()),
            None => self.record(Dir::Recv, &line),
        }
        Some(line)
    }

    /// Records a line the SDK is about to write.
    fn outbound(&self, line: &str) {
        self.record(Dir::Send, line);
    }

    fn record(&self, dir: Dir, line: &str) {
        let frame = redact::credentials(line).into_owned();
        self.raw.push(RawFrame {
            instance: self.instance.clone(),
            seq: self.last_seq.fetch_add(1, Ordering::Relaxed) + 1,
            ts: self.clock.now_wall_ms(),
            dir,
            agent_session_id: session_id(&frame),
            frame,
        });
    }
}

/// The value of the first `"sessionId"` key in `line`.
fn session_id(line: &str) -> Option<String> {
    const KEY: &str = "\"sessionId\":";
    let value = line[line.find(KEY)? + KEY.len()..].trim_start();
    let value = value.strip_prefix('"')?;
    Some(value[..value.find('"')?].to_owned())
}

/// The adapter's output as lines, each through [`Tap::inbound`]. Invalid UTF-8
/// is replaced rather than ending the connection.
fn incoming<R: AsyncRead + Send + Unpin + 'static>(
    reader: R,
    tap: Arc<Tap>,
) -> impl Stream<Item = io::Result<String>> + Send + 'static {
    futures::stream::unfold(
        (BufReader::new(reader), tap),
        |(mut reader, tap)| async move {
            loop {
                let mut buf = Vec::new();
                match reader.read_until(b'\n', &mut buf).await {
                    Ok(0) => return None,
                    Ok(_) => {
                        for end in [b'\n', b'\r'] {
                            if buf.last() == Some(&end) {
                                buf.pop();
                            }
                        }
                        let line = String::from_utf8(buf).unwrap_or_else(|err| {
                            String::from_utf8_lossy(err.as_bytes()).into_owned()
                        });
                        if let Some(line) = tap.inbound(line) {
                            return Some((Ok(line), (reader, tap)));
                        }
                    }
                    Err(err) => return Some((Err(err), (reader, tap))),
                }
            }
        },
    )
}

/// Writes the SDK's lines to the adapter, each through [`Tap::outbound`].
/// Closing it shuts the writer down, which closes the adapter's stdin.
struct LineSink<W> {
    writer: W,
    tap: Arc<Tap>,
    pending: Vec<u8>,
    written: usize,
}

impl<W: AsyncWrite + Unpin> Sink<String> for LineSink<W> {
    type Error = io::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }

    fn start_send(self: Pin<&mut Self>, line: String) -> io::Result<()> {
        let this = self.get_mut();
        this.tap.outbound(&line);
        this.pending = line.into_bytes();
        this.pending.push(b'\n');
        this.written = 0;
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        while this.written < this.pending.len() {
            let n =
                ready!(Pin::new(&mut this.writer).poll_write(cx, &this.pending[this.written..]))?;
            if n == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            this.written += n;
        }
        Pin::new(&mut this.writer).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().poll_flush(cx))?;
        Pin::new(&mut self.get_mut().writer).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::clock::ManualClock;

    fn tap() -> (Tap, mpsc::Receiver<RawFrame>) {
        let (raw, frames) = RawSink::bounded();
        let clock = Arc::new(ManualClock::starting_at(1_700_000_000_000));
        (Tap::new("instance-1", clock, raw), frames)
    }

    fn drain(frames: &mut mpsc::Receiver<RawFrame>) -> Vec<RawFrame> {
        std::iter::from_fn(|| frames.try_recv().ok()).collect()
    }

    #[test]
    fn unit_tap_numbers_every_line_in_both_directions() {
        let (tap, mut frames) = tap();
        let update = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s-1","update":{}}}"#;
        assert_eq!(tap.inbound(update.to_owned()).as_deref(), Some(update));
        assert_eq!(tap.inbound("node: a warning".to_owned()), None);
        tap.outbound(
            r#"{"jsonrpc":"2.0","id":7,"method":"session/cancel","params":{"sessionId": "s-2"}}"#,
        );

        let frames = drain(&mut frames);
        let summary: Vec<_> = frames
            .iter()
            .map(|f| (f.seq, f.dir.as_str(), f.agent_session_id.as_deref()))
            .collect();
        assert_eq!(
            summary,
            [
                (1, "recv", Some("s-1")),
                (2, "recv_nonjson", None),
                (3, "send", Some("s-2"))
            ]
        );
        assert_eq!(frames[0].frame, update);
        assert_eq!(frames[1].frame, "node: a warning");
        assert!(
            frames
                .iter()
                .all(|f| &*f.instance == "instance-1" && f.ts == 1_700_000_000_000)
        );
    }

    #[test]
    fn unit_tap_redacts_what_it_records_and_forwards_the_original() {
        let (tap, mut frames) = tap();
        let auth = json!({"jsonrpc": "2.0", "method": "_auth/status_update", "params": {"authStatus":
            {"kind": "account", "label": "Claude Max",
             "account": {"plan": "max", "email": "someone@example.com", "organization": "Example Org"}}}})
        .to_string();
        let prompt = r#"{"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{"prompt":[{"type":"text","text":"use sk-proj-0123456789abcdefXYZ"}]}}"#;
        assert_eq!(tap.inbound(auth.clone()), Some(auth));
        tap.outbound(prompt);

        let frames = drain(&mut frames);
        let recorded: Value = serde_json::from_str(&frames[0].frame).unwrap();
        assert_eq!(
            recorded["params"],
            json!({"authStatus": {"kind": "account", "label": "Claude Max", "account": {"plan": "max"}}})
        );
        assert_eq!(
            frames[1].frame,
            prompt.replace("sk-proj-0123456789abcdefXYZ", "sk-<redacted>")
        );
    }

    #[test]
    fn unit_tap_drops_and_counts_frames_when_the_queue_is_full() {
        let (raw, mut frames) = RawSink::bounded();
        let tap = Tap::new("instance-1", Arc::new(ManualClock::new()), raw.clone());
        for _ in 0..RAW_QUEUE_CAPACITY + 2 {
            tap.outbound("{}");
        }
        assert_eq!(raw.dropped(), 2);
        assert_eq!(drain(&mut frames).len(), RAW_QUEUE_CAPACITY);

        drop(frames);
        tap.outbound("{}");
        assert_eq!(raw.dropped(), 3);
    }
}
