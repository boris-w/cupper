import type { HostPort } from "@cupper/shell-api";
import { describe, expect, test } from "vitest";

import { encodeFrame, encodeJson } from "./frame";
import { handshake, type HandshakeFailure, type HandshakeOptions } from "./handshake";
import { hostError, MockHost } from "./mock/host";
import { fixtures } from "./testing";

const HOST_PROOF = "d".repeat(64);

function options(more: Partial<HandshakeOptions> = {}): HandshakeOptions {
  return {
    client: { kind: "desktop", name: "Cupper", version: "0.1.0" },
    role: "window",
    features: ["stream.coalesce"],
    auth: (hello) => ({ type: "dev_key", c_nonce: "b".repeat(64), proof: `for ${hello.nonce}` }),
    verifyHostProof: (proof) => proof === HOST_PROOF,
    refuseWeakAuth: false,
    ...more,
  };
}

/** A port whose host says `messages` (or raw frames) and nothing else. */
function scripted(...messages: (object | Uint8Array)[]): HostPort {
  return {
    send: () => undefined,
    sendBlob: () => undefined,
    onData: (cb) => {
      for (const m of messages) {
        cb((m instanceof Uint8Array ? m : encodeJson(m)).slice().buffer);
      }
      return () => undefined;
    },
    ack: () => undefined,
    close: () => undefined,
  };
}

const hello = { jsonrpc: "2.0", method: "host/hello", params: new MockHost(fixtures).hello };

describe("UNIT-hc-conn", () => {
  test("handshake: hello, then initialize with the nonce; the result without the proof", async () => {
    const host = new MockHost(fixtures);
    host.handle("initialize", ({ id, conn }) => {
      conn.reply(id, { result: host.init });
      conn.send({ jsonrpc: "2.0", method: "host/shutdown", params: {} });
      return undefined;
    });
    const conn = host.connect();
    const { handshake: done, rest } = await handshake(conn.port(), options({ coalesce_ms: 0 }));
    expect(done).toEqual({ ...host.init, host_proof: undefined });
    expect(done).not.toHaveProperty("host_proof");
    expect(rest).toEqual(encodeJson({ jsonrpc: "2.0", method: "host/shutdown", params: {} }));
    expect(conn.messages()).toEqual([
      {
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
          protocol: { min: 1, max: 1 },
          nonce: host.hello.nonce,
          client: { kind: "desktop", name: "Cupper", version: "0.1.0" },
          role: "window",
          auth: { type: "dev_key", c_nonce: "b".repeat(64), proof: `for ${host.hello.nonce}` },
          features: ["stream.coalesce"],
          coalesce_ms: 0,
        },
      },
    ]);
  });

  test("handshake: a peer needs no host proof, and a weak host passes unless refused", async () => {
    const host = new MockHost(fixtures);
    delete host.init.host_proof;
    const peer = options({ auth: () => ({ type: "peer" }), role: undefined });
    await expect(handshake(host.connect().port(), peer)).resolves.toBeDefined();
    expect(host.requests[0]?.params).not.toHaveProperty("role");
  });

  const refusing = (code: string, reason?: string) => (host: MockHost) =>
    host.handle("initialize", () => hostError(code, reason));
  const refusals: [string, (host: MockHost) => void, HandshakeFailure][] = [
    ["no common version", (h) => (h.hello.protocol = { min: 2, max: 3 }), "version_unsupported"],
    ["the host's version_unsupported", refusing("version_unsupported"), "version_unsupported"],
    ["unauthenticated", refusing("unauthenticated", "bad_key"), "unauthenticated"],
    ["forbidden", refusing("forbidden", "role"), "forbidden"],
    ["shutting down", refusing("shutting_down", "idle"), "closed"],
    ["another error", refusing("internal"), "protocol_error"],
    ["another version chosen", (h) => (h.init.protocol = 2), "protocol_error"],
    ["a malformed result", (h) => h.handle("initialize", () => ({ result: {} })), "protocol_error"],
    ["a wrong host proof", (h) => (h.init.host_proof = "e".repeat(64)), "host_unverified"],
    ["no host proof", (h) => delete h.init.host_proof, "host_unverified"],
  ];

  test.each(refusals)("handshake fails: %s", async (_, setup, reason) => {
    const host = new MockHost(fixtures);
    setup(host);
    const conn = host.connect();
    await expect(handshake(conn.port(), options())).rejects.toMatchObject({ reason });
    if (reason === "version_unsupported" && host.hello.protocol.min === 2) {
      expect(conn.messages()).toEqual([]);
    }
  });

  test("handshake fails: a release build refuses a weak host", async () => {
    const host = new MockHost(fixtures);
    expect(host.init.host.weak_auth).toBe(true);
    const port = host.connect().port();
    await expect(handshake(port, options({ refuseWeakAuth: true }))).rejects.toMatchObject({
      reason: "weak_auth",
    });
  });

  const violations: [string, HostPort][] = [
    ["a first message other than hello", scripted({ jsonrpc: "2.0", id: 1, result: {} })],
    ["a malformed hello", scripted({ ...hello, params: { nonce: 1 } })],
    ["a broken frame", scripted(new Uint8Array([0, 0, 0, 0]))],
    ["a frame that is not JSON", scripted(encodeFrame(1, new Uint8Array([0x7b])))],
    ["a notification instead of the result", scripted(hello, hello)],
    ["the result of another request", scripted(hello, { jsonrpc: "2.0", id: 2, result: {} })],
  ];

  test.each(violations)("handshake fails: %s", async (_, port) => {
    await expect(handshake(port, options())).rejects.toMatchObject({ reason: "protocol_error" });
  });

  test("handshake fails: given up while waiting", async () => {
    const abort = new AbortController();
    const waiting = handshake(scripted(), options({ signal: abort.signal }));
    abort.abort();
    await expect(waiting).rejects.toMatchObject({ reason: "closed" });
  });
});
