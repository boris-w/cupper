//! Encoding of outgoing frames, with batching that respects the frame limit.

use cupper_protocol::frame::{self, FrameError, FrameType, MAX_PAYLOAD_BYTES};
use cupper_protocol::rpc::{ErrorCode, JSONRPC_VERSION, OutError, OutNotification, OutResult, RequestId, RpcError};
use cupper_protocol::{EventBody, ResetReason, StreamResetParams};
use serde::Serialize;

pub fn response(id: &RequestId, result: &impl Serialize) -> Vec<u8> {
    frame::encode_json(&OutResult { jsonrpc: JSONRPC_VERSION, id, result }).unwrap_or_else(|e| {
        error(Some(id), &RpcError::app(ErrorCode::TooLarge, format!("response too large: {e}")))
    })
}

pub fn error(id: Option<&RequestId>, err: &RpcError) -> Vec<u8> {
    frame::encode_json(&OutError { jsonrpc: JSONRPC_VERSION, id, error: err }).expect("error frames are small")
}

pub fn reset(stream: &str, reason: ResetReason) -> Vec<u8> {
    let params = StreamResetParams { stream: stream.to_owned(), reason };
    frame::encode_json(&OutNotification { jsonrpc: JSONRPC_VERSION, method: "stream/reset", params })
        .expect("reset frames are small")
}

/// Split serialized items into contiguous pages that fit next to `overhead`
/// bytes of envelope (plus brackets and commas).
fn paginate(items: &[String], overhead: usize) -> Result<Vec<&[String]>, FrameError> {
    let base = overhead + 2;
    let mut pages = Vec::new();
    let (mut start, mut size) = (0, base);
    for (i, item) in items.iter().enumerate() {
        let add = item.len() + 1;
        if base + add > MAX_PAYLOAD_BYTES {
            return Err(FrameError::TooLarge { size: base + add, max: MAX_PAYLOAD_BYTES });
        }
        if size + add > MAX_PAYLOAD_BYTES {
            pages.push(&items[start..i]);
            (start, size) = (i, base);
        }
        size += add;
    }
    if start < items.len() || pages.is_empty() {
        pages.push(&items[start..]);
    }
    Ok(pages)
}

fn build(prefix: &str, page: &[String], suffix: &str) -> Result<Vec<u8>, FrameError> {
    let mut body = String::with_capacity(prefix.len() + suffix.len() + page.iter().map(|s| s.len() + 1).sum::<usize>() + 2);
    body.push_str(prefix);
    body.push('[');
    body.push_str(&page.join(","));
    body.push(']');
    body.push_str(suffix);
    frame::encode(FrameType::Json, body.as_bytes())
}

/// `stream/events` notifications carrying the given serialized events, in order.
pub fn events(stream: &str, events: &[String]) -> Result<Vec<Vec<u8>>, FrameError> {
    if events.is_empty() {
        return Ok(Vec::new());
    }
    let name = serde_json::to_string(stream).expect("string serializes");
    let prefix = format!(r#"{{"jsonrpc":"{JSONRPC_VERSION}","method":"stream/events","params":{{"stream":{name},"events":"#);
    paginate(events, prefix.len() + 2)?.into_iter().map(|p| build(&prefix, p, "}}")).collect()
}

/// `stream/snapshot` pages rebuilding a view at `at_seq`.
pub fn snapshot(stream: &str, at_seq: u64, ops: &[EventBody]) -> Result<Vec<Vec<u8>>, FrameError> {
    let name = serde_json::to_string(stream).expect("string serializes");
    let prefix = |page: u32, last: bool| {
        format!(
            r#"{{"jsonrpc":"{JSONRPC_VERSION}","method":"stream/snapshot","params":{{"stream":{name},"at_seq":{at_seq},"page":{page},"last":{last},"ops":"#
        )
    };
    let items: Vec<String> = ops.iter().map(|o| serde_json::to_string(o).expect("ops serialize")).collect();
    let pages = paginate(&items, prefix(u32::MAX, false).len() + 2)?;
    let n = pages.len();
    pages.into_iter().enumerate().map(|(i, p)| build(&prefix(i as u32, i + 1 == n), p, "}}")).collect()
}

/// A notification with typed params (used for small control messages).
pub fn notification<P: Serialize>(method: &str, params: P) -> Result<Vec<u8>, frame::EncodeError> {
    frame::encode_json(&OutNotification { jsonrpc: JSONRPC_VERSION, method, params })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cupper_protocol::frame::{FrameDecoder, MAX_FRAME_BYTES};
    use cupper_protocol::{StreamEventsParams, StreamSnapshotParams};

    fn decode_all(frames: &[Vec<u8>]) -> Vec<serde_json::Value> {
        let mut d = FrameDecoder::new();
        frames
            .iter()
            .map(|f| {
                assert!(f.len() <= MAX_FRAME_BYTES);
                d.push(f);
                let fr = d.next_frame().unwrap().unwrap();
                serde_json::from_slice(&fr.payload).unwrap()
            })
            .collect()
    }

    #[test]
    fn events_split_across_frames_in_order() {
        let evs: Vec<String> = (1..=50)
            .map(|i| format!(r#"{{"seq":{i},"v":1,"ts":0,"type":"item.delta","data":{{"item_id":"i","text":"{}"}}}}"#, "x".repeat(5000)))
            .collect();
        let frames = events("session/s", &evs).unwrap();
        assert!(frames.len() >= 4);
        let seqs: Vec<u64> = decode_all(&frames)
            .into_iter()
            .flat_map(|v| serde_json::from_value::<StreamEventsParams>(v["params"].clone()).unwrap().events)
            .map(|e| e.seq)
            .collect();
        assert_eq!(seqs, (1..=50).collect::<Vec<_>>());
    }

    #[test]
    fn snapshot_pages_are_numbered_and_last_is_marked() {
        let ops: Vec<EventBody> = (0..40)
            .map(|i| EventBody::ItemDelta(cupper_protocol::ItemDelta { item_id: format!("i{i}"), text: "y".repeat(10_000) }))
            .collect();
        let frames = snapshot("index", 7, &ops).unwrap();
        let pages: Vec<StreamSnapshotParams> =
            decode_all(&frames).into_iter().map(|v| serde_json::from_value(v["params"].clone()).unwrap()).collect();
        assert_eq!(pages.iter().map(|p| p.page).collect::<Vec<_>>(), (0..pages.len() as u32).collect::<Vec<_>>());
        assert!(pages.last().unwrap().last && pages.iter().rev().skip(1).all(|p| !p.last));
        assert_eq!(pages.iter().map(|p| p.ops.len()).sum::<usize>(), 40);
    }

    #[test]
    fn empty_snapshot_is_one_last_page() {
        let frames = snapshot("index", 0, &[]).unwrap();
        assert_eq!(frames.len(), 1);
    }
}
