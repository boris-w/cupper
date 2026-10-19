//! Length-prefixed framing: `u32 length (big endian) | u8 frame type | payload`.
//!
//! `length` counts the type byte plus the payload. A whole frame, including the
//! 4-byte length prefix, never exceeds [`MAX_FRAME_BYTES`], so that one frame
//! always fits into one Noise transport message (65,535 bytes minus a 16-byte tag).

use thiserror::Error;

/// Upper bound for a whole frame, length prefix included.
pub const MAX_FRAME_BYTES: usize = crate::limits::V1.max_frame_bytes as usize;
/// Length prefix plus frame type byte.
pub const FRAME_HEADER_BYTES: usize = 5;
/// Largest payload a single frame can carry.
pub const MAX_PAYLOAD_BYTES: usize = MAX_FRAME_BYTES - FRAME_HEADER_BYTES;

/// Frame type byte. Unknown values are preserved so a reader can skip them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// One JSON-RPC message encoded as UTF-8 JSON.
    Json,
    /// A blob chunk: [`BlobChunkHeader`] followed by raw bytes.
    BlobChunk,
    /// Reserved for a negotiated binary message encoding.
    Binary,
    Unknown(u8),
}

impl FrameType {
    pub fn to_byte(self) -> u8 {
        match self {
            FrameType::Json => 0x01,
            FrameType::BlobChunk => 0x02,
            FrameType::Binary => 0x03,
            FrameType::Unknown(b) => b,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0x01 => FrameType::Json,
            0x02 => FrameType::BlobChunk,
            0x03 => FrameType::Binary,
            other => FrameType::Unknown(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub frame_type: FrameType,
    pub payload: Vec<u8>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrameError {
    #[error("frame of {size} bytes exceeds the {max} byte limit")]
    TooLarge { size: usize, max: usize },
    #[error("frame length field is zero")]
    EmptyFrame,
    #[error("blob chunk header is truncated")]
    TruncatedBlobHeader,
}

/// Encode one frame. Fails instead of producing an oversized frame.
pub fn encode(frame_type: FrameType, payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    let size = FRAME_HEADER_BYTES + payload.len();
    if size > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            size,
            max: MAX_FRAME_BYTES,
        });
    }
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&((payload.len() + 1) as u32).to_be_bytes());
    out.push(frame_type.to_byte());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Serialize a message as JSON and wrap it into a `0x01` frame.
pub fn encode_json<T: serde::Serialize>(msg: &T) -> Result<Vec<u8>, EncodeError> {
    let payload = serde_json::to_vec(msg)?;
    Ok(encode(FrameType::Json, &payload)?)
}

#[derive(Debug, Error)]
pub enum EncodeError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Frame(#[from] FrameError),
}

/// Incremental decoder. Feed bytes as they arrive, then pull complete frames.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Returns the next complete frame, `Ok(None)` if more bytes are needed, or
    /// an error if the stream violates the frame limit (the connection should be closed).
    pub fn next_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if len == 0 {
            return Err(FrameError::EmptyFrame);
        }
        let size = 4 + len;
        if size > MAX_FRAME_BYTES {
            return Err(FrameError::TooLarge {
                size,
                max: MAX_FRAME_BYTES,
            });
        }
        if self.buf.len() < size {
            return Ok(None);
        }
        let frame_type = FrameType::from_byte(self.buf[4]);
        let payload = self.buf[FRAME_HEADER_BYTES..size].to_vec();
        self.buf.drain(..size);
        Ok(Some(Frame {
            frame_type,
            payload,
        }))
    }
}

/// Header of a `0x02` blob chunk frame: sha-256 of the whole blob, byte offset
/// of this chunk, and a flags byte (bit 0 = last chunk of the requested range).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobChunkHeader {
    pub hash: [u8; 32],
    pub offset: u64,
    pub last: bool,
}

impl BlobChunkHeader {
    pub const BYTES: usize = 32 + 8 + 1;
    /// Largest number of blob bytes one chunk frame can carry.
    pub const MAX_DATA: usize = MAX_PAYLOAD_BYTES - Self::BYTES;

    pub fn encode_chunk(&self, data: &[u8]) -> Result<Vec<u8>, FrameError> {
        let mut payload = Vec::with_capacity(Self::BYTES + data.len());
        payload.extend_from_slice(&self.hash);
        payload.extend_from_slice(&self.offset.to_be_bytes());
        payload.push(u8::from(self.last));
        payload.extend_from_slice(data);
        encode(FrameType::BlobChunk, &payload)
    }

    /// Split a blob chunk payload into its header and data.
    pub fn decode(payload: &[u8]) -> Result<(Self, &[u8]), FrameError> {
        if payload.len() < Self::BYTES {
            return Err(FrameError::TruncatedBlobHeader);
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&payload[..32]);
        let mut off = [0u8; 8];
        off.copy_from_slice(&payload[32..40]);
        let header = BlobChunkHeader {
            hash,
            offset: u64::from_be_bytes(off),
            last: payload[40] & 1 == 1,
        };
        Ok((header, &payload[Self::BYTES..]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn unit_proto_base_frame_rejects_oversized_payload() {
        assert!(encode(FrameType::Json, &vec![b'x'; MAX_PAYLOAD_BYTES]).is_ok());
        assert_eq!(
            encode(FrameType::Json, &vec![b'x'; MAX_PAYLOAD_BYTES + 1]),
            Err(FrameError::TooLarge {
                size: MAX_FRAME_BYTES + 1,
                max: MAX_FRAME_BYTES
            })
        );
    }

    #[test]
    fn unit_proto_base_frame_decoder_rejects_oversized_length() {
        let mut d = FrameDecoder::new();
        d.push(&(MAX_FRAME_BYTES as u32).to_be_bytes());
        assert!(matches!(d.next_frame(), Err(FrameError::TooLarge { .. })));
    }

    #[test]
    fn unit_proto_base_frame_blob_chunk_roundtrip() {
        let h = BlobChunkHeader {
            hash: [7; 32],
            offset: 1234,
            last: true,
        };
        let frame = h.encode_chunk(b"abc").unwrap();
        let mut d = FrameDecoder::new();
        d.push(&frame);
        let f = d.next_frame().unwrap().unwrap();
        assert_eq!(f.frame_type, FrameType::BlobChunk);
        let (h2, data) = BlobChunkHeader::decode(&f.payload).unwrap();
        assert_eq!(h2, h);
        assert_eq!(data, b"abc");
    }

    proptest! {
        /// Any sequence of frames survives arbitrary splitting of the byte stream.
        #[test]
        fn unit_proto_base_frame_roundtrip_with_arbitrary_splits(
            payloads in prop::collection::vec((any::<u8>(), prop::collection::vec(any::<u8>(), 0..3000)), 1..20),
            cuts in prop::collection::vec(1usize..5000, 0..40),
        ) {
            let mut wire = Vec::new();
            for (t, p) in &payloads {
                let bytes = encode(FrameType::from_byte(*t), p).unwrap();
                prop_assert!(bytes.len() <= MAX_FRAME_BYTES);
                wire.extend_from_slice(&bytes);
            }
            let mut d = FrameDecoder::new();
            let mut out = Vec::new();
            let mut pos: usize = 0;
            for c in cuts.iter().chain(std::iter::once(&usize::MAX)) {
                let end = pos.saturating_add(*c).min(wire.len());
                d.push(&wire[pos..end]);
                pos = end;
                while let Some(f) = d.next_frame().unwrap() {
                    out.push((f.frame_type.to_byte(), f.payload));
                }
                if pos == wire.len() { break; }
            }
            prop_assert_eq!(out, payloads);
        }
    }
}
