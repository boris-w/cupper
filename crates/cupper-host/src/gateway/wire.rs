//! The frames the host sends: one JSON-RPC message per `0x01` frame (design
//! 5.3.1). Pages of events and snapshots are T033's.

use cupper_protocol::error::{ErrorCode, RpcError};
use cupper_protocol::frame;
use cupper_protocol::rpc::{
    HostNotification, JSONRPC_VERSION, OutError, OutNotification, OutResult, RpcId,
};
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

/// A notification of the host. Only small ones go through here, such as
/// `host/hello` and `stream/reset`.
pub fn notification<N: HostNotification>(params: &N::Params) -> Vec<u8> {
    frame::encode_json(&OutNotification {
        jsonrpc: JSONRPC_VERSION,
        method: N::NAME,
        params,
    })
    .expect("a small notification fits into a frame")
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
