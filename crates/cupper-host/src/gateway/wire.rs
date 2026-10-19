//! The frames the host sends: one JSON-RPC message per `0x01` frame (design
//! 5.3.1). Events and snapshot ops are packed as many to a frame as fit.

use cupper_protocol::error::{ErrorCode, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::frame::{self, FrameType, MAX_FRAME_BYTES, MAX_PAYLOAD_BYTES};
use cupper_protocol::methods::stream::{Raw, StreamEvents, StreamSnapshot, StreamSnapshotParams};
use cupper_protocol::rpc::{
    HostNotification, JSONRPC_VERSION, Notification, OutError, OutNotification, OutResult, RpcId,
};
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::paginate;
use serde::Serialize;

/// Longest error message sent when the whole one does not fit into a frame.
const SHORT_MESSAGE_BYTES: usize = 1_000;

/// A response; a result too large for a frame becomes a `too_large` error.
pub fn response(id: &RpcId, result: &impl Serialize) -> Vec<u8> {
    frame::encode_json(&OutResult {
        jsonrpc: JSONRPC_VERSION,
        id,
        result,
    })
    .unwrap_or_else(|_| {
        let err = RpcError::app(ErrorCode::TooLarge, "the result does not fit into a frame");
        error(Some(id), &err)
    })
}

/// An error. Its message may quote what the client sent, and the client
/// chose the `id`: when the frame would be too large, the message is cut,
/// and if that is not enough, the `id` is left out.
pub fn error(id: Option<&RpcId>, err: &RpcError) -> Vec<u8> {
    fn encode(id: Option<&RpcId>, error: &RpcError) -> Result<Vec<u8>, frame::EncodeError> {
        frame::encode_json(&OutError {
            jsonrpc: JSONRPC_VERSION,
            id,
            error,
        })
    }
    encode(id, err).unwrap_or_else(|_| {
        let mut short = err.clone();
        let mut end = SHORT_MESSAGE_BYTES.min(short.message.len());
        while !short.message.is_char_boundary(end) {
            end -= 1;
        }
        short.message.truncate(end);
        encode(id, &short)
            .or_else(|_| encode(None, &short))
            .expect("an error with a short message and no id fits into a frame")
    })
}

/// A notification of the host that fits into a frame: a small one such as
/// `host/hello` and `stream/reset`, or a page made to fit.
pub fn notification<N: HostNotification>(params: &N::Params) -> Vec<u8> {
    frame::encode_json(&OutNotification {
        jsonrpc: JSONRPC_VERSION,
        method: N::NAME,
        params,
    })
    .expect("the notification fits into a frame")
}

/// `stream/events` of `stream` with `events`, each the JSON of an event, as
/// many to a frame as fit, in order. They stop before an event too large
/// for a frame of its own; the flag says whether all of them went in.
pub fn events<S: AsRef<str>>(stream: &StreamId, events: &[S]) -> (Vec<Vec<u8>>, bool) {
    const TAIL: &str = "]}}";
    let name = serde_json::to_string(stream).expect("a stream name serializes");
    let head = format!(
        r#"{{"jsonrpc":"{JSONRPC_VERSION}","method":"{}","params":{{"stream":{name},"events":["#,
        StreamEvents::NAME
    );
    let room = MAX_PAYLOAD_BYTES - head.len() - TAIL.len();
    let frame = |body: &str| {
        let payload = [&head, body, TAIL].concat();
        frame::encode(FrameType::Json, payload.as_bytes()).expect("the events fit into the frame")
    };
    let (mut frames, mut body) = (Vec::new(), String::new());
    for event in events.iter().map(AsRef::as_ref) {
        if event.len() > room {
            frames.extend((!body.is_empty()).then(|| frame(&body)));
            return (frames, false);
        }
        if !body.is_empty() && body.len() + 1 + event.len() > room {
            frames.push(frame(&body));
            body.clear();
        }
        if !body.is_empty() {
            body.push(',');
        }
        body.push_str(event);
    }
    frames.extend((!body.is_empty()).then(|| frame(&body)));
    (frames, true)
}

/// The `stream/snapshot` pages of `ops`, as many to a page as fit into a
/// frame, by [`paginate`]; one empty page when there are none.
pub fn snapshot(stream: &StreamId, at_seq: u64, epoch: u64, ops: &[EventBody]) -> Vec<Vec<u8>> {
    let page = |page, last, ops: &[EventBody]| StreamSnapshotParams {
        stream: stream.clone(),
        at_seq,
        epoch,
        page,
        last,
        ops: ops
            .iter()
            .map(|op| Raw::encode(op).expect("an op serializes"))
            .collect(),
    };
    // The longest envelope: the highest page number and `false`; `paginate`
    // counts the brackets of the empty `ops` it has.
    let envelope = notification::<StreamSnapshot>(&page(u32::MAX, false, &[])).len();
    let mut pages = paginate(ops, MAX_FRAME_BYTES - envelope + 2);
    if pages.is_empty() {
        pages.push(0..0);
    }
    let last = pages.len() - 1;
    let pages = pages.into_iter().enumerate().map(|(n, range)| {
        let n = u32::try_from(n).expect("fewer than 2^32 pages");
        notification::<StreamSnapshot>(&page(n, n as usize == last, &ops[range]))
    });
    pages.collect()
}

#[cfg(test)]
mod tests {
    use cupper_protocol::frame::{FrameDecoder, MAX_FRAME_BYTES};
    use serde_json::{Value, json};

    use super::*;

    fn decode(bytes: &[u8]) -> Value {
        assert!(bytes.len() <= MAX_FRAME_BYTES);
        let mut decoder = FrameDecoder::new();
        decoder.push(bytes);
        let frame = decoder.next_frame().unwrap().unwrap();
        serde_json::from_slice(&frame.payload).unwrap()
    }

    #[test]
    fn wire_oversized_errors_and_results_still_fit_into_a_frame() {
        let id = RpcId::Number(7);
        let long = format!("invalid type: string \"{}\"", "é".repeat(40_000));
        let err = RpcError::app(ErrorCode::NotFound, long.clone());
        let sent = decode(&error(Some(&id), &err));
        assert_eq!(sent["id"], 7);
        assert_eq!(sent["error"]["data"]["code"], "not_found");
        assert!(long.starts_with(sent["error"]["message"].as_str().unwrap()));

        // An id the size of a frame cannot be echoed.
        let huge = RpcId::String("i".repeat(MAX_FRAME_BYTES));
        assert_eq!(decode(&error(Some(&huge), &err))["id"], Value::Null);

        let result = json!({"text": "x".repeat(MAX_FRAME_BYTES)});
        let sent = decode(&response(&id, &result));
        assert_eq!(sent["error"]["data"]["code"], "too_large");
        assert_eq!(decode(&response(&id, &json!({})))["result"], json!({}));
    }
}
