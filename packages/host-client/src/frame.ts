// Framing (design 5.3.1): `u32 length (big endian) | u8 type | payload`, where
// the length counts the type byte and the payload, and a whole frame, prefix
// included, stays within 64,000 bytes. The same rules as `cupper_protocol::frame`.

/** One JSON-RPC message, as UTF-8 JSON. A message never spans frames. */
export const FRAME_JSON = 0x01;
/** A blob chunk: 32-byte sha-256, 8-byte offset, 1 flags byte (bit 0: last), then the bytes. */
export const FRAME_BLOB = 0x02;
/** A whole frame, length prefix included (`limits.max_frame_bytes`). */
export const MAX_FRAME_BYTES = 64_000;

const HEADER_BYTES = 5;
const BLOB_HEADER_BYTES = 41;
/** The most blob bytes one `0x02` frame carries. */
export const MAX_BLOB_CHUNK_BYTES = MAX_FRAME_BYTES - HEADER_BYTES - BLOB_HEADER_BYTES;

export interface Frame {
  type: number;
  payload: Uint8Array;
}

export interface BlobChunk {
  /** The sha-256 of the whole blob, as 64 lowercase hex digits (`BlobHash`). */
  hash: string;
  offset: number;
  /** The last chunk of the range asked for. */
  last: boolean;
  bytes: Uint8Array;
}

/** A frame that breaks the rules. On the read side the stream cannot be trusted after one. */
export class FrameError extends Error {
  override name = "FrameError";
}

export function encodeFrame(type: number, payload: Uint8Array): Uint8Array {
  const size = HEADER_BYTES + payload.length;
  if (size > MAX_FRAME_BYTES) {
    throw new FrameError(`a frame of ${size} bytes exceeds ${MAX_FRAME_BYTES}`);
  }
  const frame = new Uint8Array(size);
  new DataView(frame.buffer).setUint32(0, payload.length + 1);
  frame[4] = type;
  frame.set(payload, HEADER_BYTES);
  return frame;
}

export function encodeJson(message: unknown): Uint8Array {
  return encodeFrame(FRAME_JSON, new TextEncoder().encode(JSON.stringify(message)));
}

const HASH = /^[0-9a-f]{64}$/;

export function encodeBlobChunk({ hash, offset, last, bytes }: BlobChunk): Uint8Array {
  if (!HASH.test(hash) || !Number.isSafeInteger(offset) || offset < 0) {
    throw new FrameError("a blob chunk needs a sha-256 in hex and an offset within 2^53 - 1");
  }
  const payload = new Uint8Array(BLOB_HEADER_BYTES + bytes.length);
  for (let i = 0; i < 32; i++) {
    payload[i] = Number.parseInt(hash.slice(2 * i, 2 * i + 2), 16);
  }
  new DataView(payload.buffer).setBigUint64(32, BigInt(offset));
  payload[40] = last ? 1 : 0;
  payload.set(bytes, BLOB_HEADER_BYTES);
  return encodeFrame(FRAME_BLOB, payload);
}

export function decodeBlobChunk(payload: Uint8Array): BlobChunk {
  if (payload.length < BLOB_HEADER_BYTES) {
    throw new FrameError("a truncated blob chunk header");
  }
  const offset = new DataView(payload.buffer, payload.byteOffset).getBigUint64(32);
  if (offset > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new FrameError("a blob offset beyond 2^53 - 1");
  }
  const hash = Array.from(payload.subarray(0, 32), (b) => b.toString(16).padStart(2, "0"));
  return {
    hash: hash.join(""),
    offset: Number(offset),
    last: ((payload[40] ?? 0) & 1) === 1,
    bytes: payload.slice(BLOB_HEADER_BYTES),
  };
}

/** Cuts frames out of byte chunks that may split them anywhere. */
export class FrameDecoder {
  #buffer = new Uint8Array(0);

  push(chunk: Uint8Array): void {
    const joined = new Uint8Array(this.#buffer.length + chunk.length);
    joined.set(this.#buffer);
    joined.set(chunk, this.#buffer.length);
    this.#buffer = joined;
  }

  /** The next whole frame; `undefined` until more bytes arrive. Throws `FrameError`. */
  next(): Frame | undefined {
    const buffer = this.#buffer;
    if (buffer.length < 4) {
      return undefined;
    }
    const length = new DataView(buffer.buffer, buffer.byteOffset).getUint32(0);
    if (length === 0) {
      throw new FrameError("a frame of length 0");
    }
    // Checked before the frame arrives, as the Rust decoder does.
    if (4 + length > MAX_FRAME_BYTES) {
      throw new FrameError(`a frame of ${4 + length} bytes exceeds ${MAX_FRAME_BYTES}`);
    }
    if (buffer.length < 4 + length) {
      return undefined;
    }
    this.#buffer = buffer.subarray(4 + length);
    return { type: buffer[4] ?? 0, payload: buffer.slice(HEADER_BYTES, 4 + length) };
  }

  /** The bytes not cut into frames yet. */
  rest(): Uint8Array {
    return this.#buffer.slice();
  }
}
