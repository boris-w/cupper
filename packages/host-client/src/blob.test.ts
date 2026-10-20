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
