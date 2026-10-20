import {
  decodeBlobChunk,
  FRAME_BLOB,
  FrameDecoder,
  MAX_BLOB_CHUNK_BYTES,
  MAX_FRAME_BYTES,
} from "@cupper/host-client";
import { MockHost, type MockConnection } from "@cupper/host-client/mock";
import { CLIENT_NOTIFICATIONS, METHODS } from "@cupper/protocol";
import { describe, expect, test } from "vitest";

import type { ToWindow } from "../../shared/ipc";
import { fixtures, mockConnect, settle, wired } from "./testing";
import {
  allowed,
  MAX_UNACKED_BYTES,
  WindowBridge,
  type BridgeDeps,
  type WindowPort,
} from "./window-bridge";

/** A bridge whose window end is in the test's hands, with its timers. */
function bridged(host: MockHost, deps: Partial<BridgeDeps> = {}) {
  const posted: ToWindow[] = [];
  let send: (message: unknown) => void = () => undefined;
  const timers: (() => void)[] = [];
  const window: WindowPort = {
    post: (message) => posted.push(message),
    onMessage: (cb) => (send = cb),
    close: () => posted.push({ type: "state", state: { status: "failed", reason: "closed" } }),
  };
  const conns: Awaited<ReturnType<typeof mockConnect>>[] = [];
  const bridge = new WindowBridge(window, {
    connect: async () => {
      const conn = await mockConnect(host);
      conns.push(conn);
      return conn;
    },
    controlReady: async () => undefined,
    schedule: (task) => void timers.push(task),
    ...deps,
  });
  const flush = () => timers.splice(0).forEach((task) => task());
  const messages = () => {
    const decoder = new FrameDecoder();
    posted.forEach((m) => m.type === "data" && decoder.push(new Uint8Array(m.chunk)));
    const out: unknown[] = [];
    for (let frame = decoder.next(); frame; frame = decoder.next()) {
      out.push(JSON.parse(new TextDecoder().decode(frame.payload)));
    }
    return out;
  };
  return { bridge, posted, send: (m: unknown) => send(m), flush, conns, messages };
}

const conn = (host: MockHost) => host.connections.at(-1) as MockConnection;

describe("UNIT-bridge-whitelist", () => {
  test("only read, write, approve and ui_local methods pass; the rest stay local", () => {
    const passing = Object.entries(METHODS)
      .filter(([method]) => allowed(method, true))
      .map(([, info]) => info.scope);
    expect(new Set(passing)).toEqual(new Set(["read", "write", "approve", "ui_local"]));
    for (const method of ["initialize", "ping", "host/exit", "test/seed", "no/such", "toString"]) {
      expect(allowed(method, true), method).toBe(false);
    }
    // Notifications are the client notifications only.
    expect(Object.keys(CLIENT_NOTIFICATIONS).every((m) => allowed(m, false))).toBe(true);
    expect(allowed("host/status", false)).toBe(false);
  });

  test("every method of the catalog reaches the host or is refused with forbidden", async () => {
    const host = new MockHost(fixtures);
    const { bridge, send, flush, messages } = bridged(host);
    await bridge.open();
    const methods = Object.keys(METHODS);
    methods.forEach((method, id) =>
      send({ type: "send", json: JSON.stringify({ jsonrpc: "2.0", id, method }) }),
    );
    await settle();
    flush();
    const reached = conn(host)
      .messages()
      .slice(1)
      .map((m) => (m as { method: string }).method);
    expect(reached).toEqual(methods.filter((m) => allowed(m, true)));
    const refused = messages().filter(
      (m) => (m as { error?: { data: { code: string } } }).error?.data.code === "forbidden",
    );
    expect(refused.map((m) => methods[(m as { id: number }).id])).toEqual(
      methods.filter((m) => !allowed(m, true)),
    );
  });

  test("malformed messages, unsafe integers and unknown window messages go nowhere", async () => {
    const host = new MockHost(fixtures);
    const { bridge, send, flush, messages } = bridged(host);
    await bridge.open();
    const before = conn(host).frames.length;
    const json = (value: unknown) => ({ type: "send", json: JSON.stringify(value) });
    for (const message of [
      json({ jsonrpc: "2.0", id: 1, method: "host/status", params: { n: 2 ** 53 } }),
      json({ jsonrpc: "2.0", id: 2, result: {} }),
      json({ jsonrpc: "2.0", method: "session/send" }),
      {
        type: "send",
        json: '{"jsonrpc":"2.0","id":3,"method":"host/status","params":{"n":9007199254740993}}',
      },
      { type: "send", json: 5 },
      { type: "raw", bytes: new ArrayBuffer(8) },
      { type: "blob", hash: "zz", offset: 0, last: true, bytes: new ArrayBuffer(1) },
      {
        type: "blob",
        hash: "ab".repeat(32),
        offset: 0,
        last: true,
        bytes: new ArrayBuffer(MAX_BLOB_CHUNK_BYTES + 1),
      },
      "close",
    ]) {
      send(message);
    }
    await settle();
    flush();
    expect(conn(host).frames.length).toBe(before);
    expect(messages()).toMatchObject([
      { id: 1, error: { data: { code: "forbidden" } } },
      { id: 3 },
    ]);
  });

  test("uploads stop at the pending limit until blob/put takes them", async () => {
    const host = new MockHost(fixtures);
    host.init.limits.max_pending_upload_bytes = 10;
    const { bridge, send } = bridged(host);
    await bridge.open();
    const blob = (hash: string, n: number) =>
      send({ type: "blob", hash, offset: 0, last: true, bytes: new ArrayBuffer(n) });
    const [a, b] = ["ab".repeat(32), "cd".repeat(32)];
    blob(a, 6);
    blob(b, 6);
    send({
      type: "send",
      json: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "blob/put", params: { hash: a } }),
    });
    blob(b, 6);
    await settle();
    const blobs = conn(host)
      .frames.filter((f) => f.type === FRAME_BLOB)
      .map((f) => decodeBlobChunk(f.payload).hash);
    expect(blobs).toEqual([a, b]);
  });
});

describe("UNIT-bridge-flow", () => {
  test("inbound frames go to the window together, one ArrayBuffer per batch", async () => {
    const host = new MockHost(fixtures);
    const { bridge, posted, flush } = bridged(host);
    await bridge.open();
    host.notify("host/shutdown", { reason: "idle", restart_expected: false });
    host.notify("host/shutdown", { reason: "idle", restart_expected: false });
    expect(posted).toEqual([]);
    flush();
    expect(posted).toHaveLength(1);
    expect(posted[0]?.type === "data" && posted[0].chunk).toBeInstanceOf(ArrayBuffer);
  });

  test("past 8 MB unacknowledged the bridge stops reading, and reads again after the ack", async () => {
    const host = new MockHost(fixtures);
    const { bridge, posted, send, flush, conns } = bridged(host);
    await bridge.open();
    const bytes = new Uint8Array(MAX_BLOB_CHUNK_BYTES);
    // Whole frames of 64,000 bytes, as many as fit under the limit.
    const chunks = Math.floor(MAX_UNACKED_BYTES / MAX_FRAME_BYTES);
    for (let i = 0; i < chunks; i++) {
      conn(host).sendBlob({ hash: "ab".repeat(32), offset: i, last: false, bytes });
      flush();
    }
    expect(conns[0]?.paused()).toBe(false);
    conn(host).sendBlob({ hash: "ab".repeat(32), offset: chunks, last: true, bytes });
    expect(conns[0]?.paused()).toBe(true);
    flush();
    const delivered = posted.reduce((n, m) => n + (m.type === "data" ? m.chunk.byteLength : 0), 0);
    send({ type: "ack", bytes: delivered - MAX_UNACKED_BYTES });
    expect(conns[0]?.paused()).toBe(false);
  });
});

describe("UNIT-bridge-lifecycle", () => {
  test("a reconnect waits for the control connection; states and data keep their order", async () => {
    const host = new MockHost(fixtures);
    let controlBack: () => void = () => undefined;
    const controlReady = () => new Promise<void>((resolve) => (controlBack = resolve));
    const { bridge, posted, flush } = bridged(host, { controlReady });
    await bridge.open();
    conn(host).send({ jsonrpc: "2.0", method: "host/shutdown", params: { reason: "upgrade" } });
    conn(host).close();
    await settle();
    expect(posted.map((m) => m.type === "data" || m.state.status)).toEqual([true, "reconnecting"]);
    expect(host.connections).toHaveLength(1);
    controlBack();
    await settle();
    host.notify("host/shutdown", { reason: "idle" });
    flush();
    expect(posted.map((m) => m.type === "data" || m.state.status)).toEqual([
      true,
      "reconnecting",
      "ready",
      true,
    ]);
    expect(posted[2]).toMatchObject({ handshake: { host: host.init.host } });
  });

  test("a reconnect that fails says why", async () => {
    const host = new MockHost(fixtures);
    const { bridge, posted } = bridged(host, {
      controlReady: () => Promise.reject(Object.assign(new Error(), { reason: "data_too_new" })),
    });
    await bridge.open();
    conn(host).close();
    await settle();
    expect(posted.at(-1)).toEqual({
      type: "state",
      state: { status: "failed", reason: "data_too_new" },
    });
  });

  test("disconnect drops the connection and the bridge makes another; close ends it for good", async () => {
    const host = new MockHost(fixtures);
    const { shell, bridges } = wired(host);
    const states: string[] = [];
    shell.onConnectionState((state) => states.push(state.status));
    await shell.open();
    bridges[0]?.disconnect();
    await settle();
    expect(states).toEqual(["reconnecting", "ready"]);
    expect(host.connections.map((c) => c.closed)).toEqual([true, false]);
    bridges[0]?.close();
    await settle();
    expect(host.connections.every((c) => c.closed)).toBe(true);
    expect(states).toEqual(["reconnecting", "ready"]);
  });
});
