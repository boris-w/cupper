import { describe, expect, test } from "vitest";

import {
  decodeBlobChunk,
  encodeBlobChunk,
  encodeFrame,
  encodeJson,
  FRAME_BLOB,
  FRAME_JSON,
  FrameDecoder,
  FrameError,
  MAX_BLOB_CHUNK_BYTES,
  MAX_FRAME_BYTES,
} from "./frame";

const text = (payload: Uint8Array) => new TextDecoder().decode(payload);

describe("UNIT-hc-conn", () => {
  test("frames: a JSON frame survives any split of its bytes", () => {
    const messages = [{ jsonrpc: "2.0", method: "ping", id: 1 }, { text: "中文 ✓" }];
    const bytes = new Uint8Array([...encodeJson(messages[0]), ...encodeJson(messages[1])]);
    const first = JSON.stringify(messages[0]).length;
    expect([...bytes.subarray(0, 5)]).toEqual([0, 0, 0, first + 1, FRAME_JSON]);
    for (const size of [1, 3, 7, bytes.length]) {
      const decoder = new FrameDecoder();
      const got: unknown[] = [];
      for (let at = 0; at < bytes.length; at += size) {
        decoder.push(bytes.subarray(at, at + size));
        for (let frame = decoder.next(); frame; frame = decoder.next()) {
          got.push(JSON.parse(text(frame.payload)));
        }
      }
      expect(got).toEqual(messages);
      expect(decoder.rest()).toEqual(new Uint8Array(0));
    }
  });

  test("frames: a 0x02 chunk keeps its hash, offset, flag and bytes", () => {
    const chunk = { hash: "0f".repeat(31) + "a9", offset: 2 ** 53 - 1, last: true };
    const bytes = new Uint8Array([1, 2, 3]);
    const frame = new FrameDecoder();
    frame.push(encodeBlobChunk({ ...chunk, bytes }));
    const decoded = frame.next();
    expect(decoded?.type).toBe(FRAME_BLOB);
    expect(decodeBlobChunk(decoded?.payload ?? new Uint8Array())).toEqual({ ...chunk, bytes });
    for (const bad of [
      { hash: "0F".repeat(32) },
      { hash: "0f" },
      { offset: -1 },
      { offset: 0.5 },
    ]) {
      expect(() => encodeBlobChunk({ ...chunk, bytes, ...bad })).toThrow(FrameError);
    }
    const header = encodeBlobChunk({ ...chunk, offset: 0, bytes }).slice(5, 46);
    header.fill(0xff, 32, 40);
    expect(() => decodeBlobChunk(header)).toThrow("beyond 2^53");
    expect(() => decodeBlobChunk(header.subarray(0, 40))).toThrow("truncated");
  });

  test("frames: 64,000 bytes is the most a frame takes, either way", () => {
    const largest = encodeFrame(FRAME_JSON, new Uint8Array(MAX_FRAME_BYTES - 5));
    expect(largest.length).toBe(MAX_FRAME_BYTES);
    expect(() => encodeFrame(FRAME_JSON, new Uint8Array(MAX_FRAME_BYTES - 4))).toThrow(FrameError);
    const bytes = new Uint8Array(MAX_BLOB_CHUNK_BYTES + 1);
    expect(() => encodeBlobChunk({ hash: "00".repeat(32), offset: 0, last: false, bytes })).toThrow(
      FrameError,
    );

    const decoder = new FrameDecoder();
    decoder.push(largest);
    expect(decoder.next()?.payload.length).toBe(MAX_FRAME_BYTES - 5);
    // The prefix alone gives an oversized frame away; so does a length of 0.
    for (const prefix of [
      [0, 0, 0xf9, 0xfd],
      [0, 0, 0, 0],
    ]) {
      const decoder = new FrameDecoder();
      decoder.push(new Uint8Array(prefix));
      expect(() => decoder.next()).toThrow(FrameError);
    }
  });

  test("frames: an unknown frame type is cut like any other", () => {
    const decoder = new FrameDecoder();
    decoder.push(encodeFrame(0x03, new Uint8Array([9])));
    decoder.push(new Uint8Array([0, 0]));
    expect(decoder.next()).toEqual({ type: 0x03, payload: new Uint8Array([9]) });
    expect(decoder.next()).toBeUndefined();
    expect(decoder.rest()).toEqual(new Uint8Array([0, 0]));
  });
});
