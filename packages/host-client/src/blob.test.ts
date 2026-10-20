import { createHash } from "node:crypto";

import { describe, expect, test } from "vitest";

import { ConnectionLost, HostClient, RpcFailure } from "./client";
import { decodeBlobChunk, FRAME_BLOB, MAX_BLOB_CHUNK_BYTES, MAX_FRAME_BYTES } from "./frame";
import { handshake } from "./handshake";
import { hostError, MockHost } from "./mock/host";
import { MOCK_HANDSHAKE } from "./mock/port";
import { fixtures } from "./testing";

/** A client on a mock host, past the handshake. */
async function connected() {
  const host = new MockHost(fixtures);
  const conn = host.connect();
  const { handshake: done, rest } = await handshake(conn.port(), MOCK_HANDSHAKE);
  const client = new HostClient();
  client.attach(conn.port(), done, rest);
  const chunks = () => conn.frames.filter((f) => f.type === FRAME_BLOB);
  return { host, client, chunks };
}

describe("UNIT-composer-long-text: HostClient.putBlob", () => {
  test("chunks of at most a frame, in order, the last marked; then blob/put", async () => {
    const { host, client, chunks } = await connected();
    const bytes = new Uint8Array(150_000).map((_, n) => n % 251);
    const hash = createHash("sha256").update(bytes).digest("hex");
    const blob = await client.putBlob(bytes, "text/plain");
    expect(blob).toEqual({ hash, size: 150_000, media_type: "text/plain" });
    const sent = chunks().map((frame) => decodeBlobChunk(frame.payload));
    expect(sent.map((c) => [c.hash, c.offset, c.last])).toEqual([
      [hash, 0, false],
      [hash, MAX_BLOB_CHUNK_BYTES, false],
      [hash, 2 * MAX_BLOB_CHUNK_BYTES, true],
    ]);
    expect(chunks().every((f) => f.payload.length + 5 <= MAX_FRAME_BYTES)).toBe(true);
    expect(Buffer.concat(sent.map((c) => c.bytes))).toEqual(Buffer.from(bytes));
    const put = host.requests.find((r) => r.method === "blob/put");
    expect(put?.params).toEqual(blob);
  });

  test("an empty blob is one chunk, marked last", async () => {
    const { client, chunks } = await connected();
    await client.putBlob(new Uint8Array(0), "text/plain");
    const sent = chunks().map((frame) => decodeBlobChunk(frame.payload));
    expect(sent.map((c) => [c.offset, c.last, c.bytes.length])).toEqual([[0, true, 0]]);
  });

  test("the host's refusals come with their codes; no connection, no put", async () => {
    const { host, client } = await connected();
    for (const code of ["conflict", "too_large"]) {
      host.handle("blob/put", () => hostError(code));
      const refused = await client
        .putBlob(new Uint8Array([1]), "text/plain")
        .catch((e: unknown) => e);
      expect(refused).toBeInstanceOf(RpcFailure);
      expect((refused as RpcFailure).error.data).toMatchObject({ code });
    }
    const idle = new HostClient();
    await expect(idle.putBlob(new Uint8Array([1]), "text/plain")).rejects.toBeInstanceOf(
      ConnectionLost,
    );
  });
});

describe("UNIT-blob-fulltext: HostClient.getBlob", () => {
  /** A host whose `blob/get` sends `pieces` (offset, bytes) first, then its result. */
  async function serving(
    bytes: Uint8Array,
    pieces: [number, Uint8Array][],
    chunks = pieces.length,
  ) {
    const { host, client } = await connected();
    const hash = createHash("sha256").update(bytes).digest("hex");
    host.handle("blob/get", ({ conn }) => {
      pieces.forEach(([offset, piece], n) =>
        conn.sendBlob({ hash, offset, last: n === pieces.length - 1, bytes: piece }),
      );
      return { result: { hash, chunks, offset: 0, length: bytes.length, size: bytes.length } };
    });
    return { client, hash };
  }

  test("joins chunks by offset, in whatever order they came", async () => {
    const bytes = new Uint8Array(100_000).map((_, n) => n % 253);
    const head = bytes.slice(0, MAX_BLOB_CHUNK_BYTES);
    const tail = bytes.slice(MAX_BLOB_CHUNK_BYTES);
    const { client, hash } = await serving(bytes, [
      [MAX_BLOB_CHUNK_BYTES, tail],
      [0, head],
    ]);
    expect(Buffer.from(await client.getBlob({ hash }))).toEqual(Buffer.from(bytes));
  });

  test("an empty blob is one empty chunk", async () => {
    const empty = new Uint8Array(0);
    const { client, hash } = await serving(empty, [[0, empty]]);
    expect((await client.getBlob({ hash })).length).toBe(0);
  });

  test("too few chunks, a gap or other bytes reject", async () => {
    const bytes = new Uint8Array(70_000).fill(7);
    const head = bytes.slice(0, MAX_BLOB_CHUNK_BYTES);
    const short = await serving(bytes, [[0, head]], 2);
    await expect(short.client.getBlob({ hash: short.hash })).rejects.toThrow(/of 2 chunks/);
    const other = await serving(bytes, [
      [0, head],
      [MAX_BLOB_CHUNK_BYTES, new Uint8Array(bytes.length - MAX_BLOB_CHUNK_BYTES).fill(8)],
    ]);
    await expect(other.client.getBlob({ hash: other.hash })).rejects.toThrow("do not match");
  });
});
