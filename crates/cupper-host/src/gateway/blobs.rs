//! `blob/put` and `blob/get` (design 5.3.1, 5.3.4, 5.3.9), which the gateway
//! serves itself. A connection's `0x02` chunks gather in its [`Uploads`], which
//! go with the connection; `blob/put` takes a blob's chunks out and stores
//! them through the [`BlobStore`], which checks the hash. `blob/get` sends the
//! range asked for as chunks ahead of its result.

use std::collections::HashMap;
use std::sync::Arc;

use cupper_protocol::error::{ErrorCode, RpcError};
use cupper_protocol::frame::BlobChunkHeader;
use cupper_protocol::ids::BlobHash;
use cupper_protocol::limits::V1;
use cupper_protocol::methods::stream::{
    BlobGetParams, BlobGetResult, BlobPutParams, BlobPutResult,
};
use serde_json::json;

use super::dispatch::internal;
use super::outbox::Outbox;
use crate::ledger::Outcome;
use crate::store::blobs::{BlobError, BlobStore};

/// Blobs a connection may have chunks of at once. Each holds an entry until
/// `blob/put` or the end of the connection, bytes or not (an upload that
/// ended past a limit or at a gap, a zero-length chunk); without a cap a
/// client with `write` grows the host by some 250 bytes a 46-byte frame (T176).
const MAX_PENDING_UPLOADS: usize = 64;

/// The chunks a connection sent and has not put yet: each blob within
/// `max_blob_bytes`, all of them within `max_pending_upload_bytes`, at most
/// [`MAX_PENDING_UPLOADS`] blobs; a chunk of one more blob is dropped.
#[derive(Debug)]
pub(super) struct Uploads {
    blobs: HashMap<[u8; 32], Upload>,
    bytes: u64,
    /// One blob's bytes, and all of them.
    limits: (u64, u64),
}

impl Default for Uploads {
    fn default() -> Self {
        let one = u64::from(V1.max_blob_bytes);
        let (blobs, bytes) = (HashMap::new(), 0);
        let limits = (one, u64::from(V1.max_pending_upload_bytes));
        Self {
            blobs,
            bytes,
            limits,
        }
    }
}

/// What `blob/put` finds of its blob.
#[derive(Debug, Default, PartialEq)]
pub(super) enum Upload {
    #[default]
    Empty,
    Bytes(Vec<u8>),
    /// A chunk would have passed a limit; the upload grew no more.
    TooLarge,
    /// A chunk did not start where the bytes so far end.
    OutOfOrder,
}

impl Uploads {
    /// A chunk of `header.hash` at `header.offset`. One that passes a limit or
    /// leaves a gap ends the upload, which then holds no bytes.
    pub fn chunk(&mut self, header: &BlobChunkHeader, data: &[u8]) {
        let Self {
            blobs,
            bytes,
            limits: (one, all),
        } = self;
        if blobs.len() >= MAX_PENDING_UPLOADS && !blobs.contains_key(&header.hash) {
            return;
        }
        let upload = blobs.entry(header.hash).or_default();
        let held = match upload {
            Upload::Empty => 0,
            Upload::Bytes(held) => held.len() as u64,
            Upload::TooLarge | Upload::OutOfOrder => return,
        };
        let more = data.len() as u64;
        let ended = if held + more > *one || *bytes + more > *all {
            Some(Upload::TooLarge)
        } else {
            (header.offset != held).then_some(Upload::OutOfOrder)
        };
        if let Some(ended) = ended {
            *bytes -= held;
            *upload = ended;
        } else if let Upload::Bytes(held) = upload {
            held.extend_from_slice(data);
            *bytes += more;
        } else {
            *upload = Upload::Bytes(data.to_vec());
            *bytes += more;
        }
    }

    /// The upload of `hash`, out of the buffer.
    pub fn take(&mut self, hash: &BlobHash) -> Upload {
        let upload = raw(hash).and_then(|raw| self.blobs.remove(&raw));
        let upload = upload.unwrap_or_default();
        if let Upload::Bytes(held) = &upload {
            self.bytes -= held.len() as u64;
        }
        upload
    }
}

/// The sha-256 a blob hash is the hex of.
fn raw(hash: &BlobHash) -> Option<[u8; 32]> {
    let hex = hash.as_str().as_bytes();
    let mut raw = [0; 32];
    if hex.len() != 64 {
        return None;
    }
    for (byte, pair) in raw.iter_mut().zip(hex.chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(raw)
}

fn conflict(message: &str) -> RpcError {
    RpcError::app(ErrorCode::Conflict, message)
}

fn too_large(max: u32) -> RpcError {
    RpcError::app(
        ErrorCode::TooLarge,
        format!("a blob has at most {max} bytes"),
    )
}

/// `blob/put`: `upload` is what the connection sent of the blob.
pub(super) async fn put(store: BlobStore, params: BlobPutParams, upload: Upload) -> Outcome {
    let data = match upload {
        _ if params.size > u64::from(V1.max_blob_bytes) => {
            return Err(too_large(V1.max_blob_bytes));
        }
        Upload::TooLarge => return Err(too_large(V1.max_pending_upload_bytes)),
        Upload::OutOfOrder => return Err(conflict("the chunks left a gap")),
        Upload::Empty => Vec::new(),
        Upload::Bytes(data) => data,
    };
    if data.len() as u64 != params.size {
        return Err(conflict("the chunks do not add up to the size"));
    }
    match store.put(data, params.media_type, Some(params.hash)).await {
        Ok(blob) => Ok(json!(BlobPutResult { hash: blob.hash })),
        Err(BlobError::Mismatch) => Err(conflict("the content does not match the hash")),
        Err(BlobError::TooLarge { .. }) => Err(too_large(V1.max_blob_bytes)),
        Err(err) => Err(internal(err)),
    }
}

/// `blob/get`: the range as chunks into `outbox`, at least one, the last
/// marked; then the result.
pub(super) async fn get(store: BlobStore, outbox: Arc<Outbox>, params: BlobGetParams) -> Outcome {
    let data = match store.get(&params.hash).await {
        Ok(data) => data,
        Err(BlobError::NotFound) => return Err(RpcError::app(ErrorCode::NotFound, "no such blob")),
        Err(err) => return Err(internal(err)),
    };
    let size = data.len() as u64;
    let offset = params.offset.unwrap_or(0).min(size);
    let length = params.length.unwrap_or(size).min(size - offset);
    let range = &data[offset as usize..(offset + length) as usize];
    let pieces: Vec<&[u8]> = match range.len() {
        0 => vec![range],
        _ => range.chunks(BlobChunkHeader::MAX_DATA).collect(),
    };
    let hash = raw(&params.hash).ok_or_else(|| internal("a stored blob's hash is hex"))?;
    for (n, piece) in pieces.iter().enumerate() {
        let at = offset + (n * BlobChunkHeader::MAX_DATA) as u64;
        let header = BlobChunkHeader {
            hash,
            offset: at,
            last: n + 1 == pieces.len(),
        };
        outbox.push_frame(header.encode_chunk(piece).map_err(internal)?);
    }
    let chunks = pieces.len() as u32;
    Ok(json!(BlobGetResult {
        hash: params.hash,
        size,
        offset,
        length,
        chunks
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uploads_gather_in_order_and_end_past_a_limit_or_at_a_gap() {
        let mut uploads = Uploads {
            limits: (10, 15),
            ..Uploads::default()
        };
        let mut chunk = |n: u8, offset: u64, data: &[u8]| {
            let header = BlobChunkHeader {
                hash: [n; 32],
                offset,
                last: false,
            };
            uploads.chunk(&header, data);
        };
        chunk(1, 0, b"abc");
        chunk(1, 3, b"de");
        chunk(2, 0, b"abc");
        chunk(2, 4, b"x");
        chunk(3, 0, &[0; 8]);
        chunk(3, 8, &[0; 3]);
        chunk(3, 8, &[0; 1]);
        let hash = |n: u8| BlobHash::from_sha256(&[n; 32]);
        assert_eq!(uploads.take(&hash(1)), Upload::Bytes(b"abcde".to_vec()));
        assert_eq!(uploads.take(&hash(1)), Upload::Empty);
        assert_eq!(uploads.take(&hash(2)), Upload::OutOfOrder);
        assert_eq!(
            uploads.take(&hash(3)),
            Upload::TooLarge,
            "one blob past 10 bytes"
        );
        // All of them past 15 bytes; a put makes room again.
        let mut chunk = |n: u8, data: &[u8]| {
            let header = BlobChunkHeader {
                hash: [n; 32],
                offset: 0,
                last: true,
            };
            uploads.chunk(&header, data);
        };
        chunk(4, &[0; 9]);
        chunk(5, &[0; 7]);
        assert_eq!(uploads.take(&hash(5)), Upload::TooLarge);
        assert_eq!(uploads.take(&hash(4)), Upload::Bytes(vec![0; 9]));
        uploads.chunk(
            &BlobChunkHeader {
                hash: [5; 32],
                offset: 0,
                last: true,
            },
            &[0; 7],
        );
        assert_eq!(uploads.take(&hash(5)), Upload::Bytes(vec![0; 7]));
        assert_eq!(uploads.bytes, 0);
    }

    #[test]
    fn uploads_hold_at_most_max_pending_blobs_even_without_bytes() {
        let mut uploads = Uploads::default();
        let header = |n: usize, offset: u64| BlobChunkHeader {
            hash: [u8::try_from(n).unwrap(); 32],
            offset,
            last: false,
        };
        let hash = |n: usize| BlobHash::from_sha256(&[u8::try_from(n).unwrap(); 32]);
        // A gap at the first chunk, or no bytes: entries all the same.
        for n in 0..MAX_PENDING_UPLOADS {
            uploads.chunk(&header(n, n as u64 % 2), &[]);
        }
        let more = MAX_PENDING_UPLOADS;
        uploads.chunk(&header(more, 0), b"x");
        assert_eq!(
            uploads.take(&hash(more)),
            Upload::Empty,
            "one blob more is dropped"
        );
        assert_eq!(uploads.take(&hash(1)), Upload::OutOfOrder);
        uploads.chunk(&header(more, 0), b"x");
        assert_eq!(uploads.take(&hash(more)), Upload::Bytes(b"x".to_vec()));
        assert_eq!(uploads.take(&hash(0)), Upload::Bytes(Vec::new()));
    }
}
