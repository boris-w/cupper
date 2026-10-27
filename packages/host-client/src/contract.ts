// The HostPort contract (design 5.7.3): what every `ShellAPI.host` and its
// HostPort do, both the browser mock and the windowBridge with the preload
// (T076). Call it inside a `describe` named by the test ID.

import type { HostPort, ShellAPI } from "@cupper/shell-api";
import { expect, test, vi } from "vitest";

import { decodeBlobChunk, FRAME_BLOB, FrameDecoder } from "./frame";
import type { MockConnection, MockHost } from "./mock/host";

export interface HostPortFixture {
  /** The implementation under test, in front of `host`. */
  shell: ShellAPI["host"];
  host: MockHost;
}

const STATUS = '{"jsonrpc":"2.0","id":7,"method":"host/status","params":{}}';
const SHUTDOWN = { reason: "idle", restart_expected: false };

/**
 * Until `check` passes. What crosses a channel takes as long as the machine
 * lets it, so the cases wait for what they expect, not for a time; the limit
 * only ends a case that would wait for ever, and it fails with the last
 * check's message, which says what was awaited.
 */
const until = (check: () => void) => vi.waitFor(check, { timeout: 5000, interval: 5 });

export function hostPortContract(setup: () => Promise<HostPortFixture>): void {
  async function opened() {
    const fixture = await setup();
    const { port, handshake } = await fixture.shell.open();
    const conn = fixture.host.connections.at(-1) as MockConnection;
    return { ...fixture, port, handshake, conn };
  }

  /** What `port` delivers: the chunks, and the messages cut out of them. */
  function inbound(port: HostPort, seen: unknown[] = []) {
    const decoder = new FrameDecoder();
    const chunks: unknown[] = [];
    const off = port.onData((chunk) => {
      chunks.push(chunk);
      decoder.push(new Uint8Array(chunk));
      for (let frame = decoder.next(); frame; frame = decoder.next()) {
        seen.push(JSON.parse(new TextDecoder().decode(frame.payload)));
      }
    });
    return { messages: seen, chunks, off };
  }

  /**
   * A request behind what was sent before it, and its answer: once it is
   * back, all of that has been handled, so what did not arrive by then
   * never will.
   */
  async function probe(host: MockHost, port: HostPort, got: { messages: unknown[] }) {
    host.handle("host/status", () => ({ result: { ok: true } }));
    port.send(STATUS);
    await until(() => expect(got.messages.at(-1)).toMatchObject({ id: 7, result: { ok: true } }));
  }

  test("open hands over the port after one window handshake, without the host's proof", async () => {
    const { host, handshake, port } = await opened();
    const inits = host.requests.filter((r) => r.method === "initialize");
    expect(inits.map((r) => (r.params as { role?: unknown }).role)).toEqual(["window"]);
    expect(handshake).not.toHaveProperty("host_proof");
    expect(handshake.principal).toEqual(host.init.principal);
    port.close();
  });

  test("a request reaches the host re-serialized, and its response comes back", async () => {
    const { host, port, conn } = await opened();
    host.handle("host/status", () => ({ result: { ok: true } }));
    const got = inbound(port);
    port.send(STATUS.replace('"params":{}', '"params":{"a":1},"params":{}'));
    await until(() =>
      expect(got.messages).toEqual([{ jsonrpc: "2.0", id: 7, result: { ok: true } }]),
    );
    expect(new TextDecoder().decode(conn.frames.at(-1)?.payload)).toBe(STATUS);
    port.close();
  });

  test("initialize is answered with forbidden/already_initialized and stays local", async () => {
    const { host, port, conn } = await opened();
    const got = inbound(port);
    const before = conn.frames.length;
    port.send('{"jsonrpc":"2.0","id":9,"method":"initialize","params":{}}');
    const refused = { code: -32000, data: { code: "forbidden", reason: "already_initialized" } };
    await until(() => expect(got.messages).toMatchObject([{ id: 9, error: refused }]));
    await probe(host, port, got);
    expect(conn.frames).toHaveLength(before + 1);
    port.close();
  });

  test("batch arrays, other JSON values, broken JSON and initialize without id are dropped", async () => {
    const { host, port, conn } = await opened();
    const got = inbound(port);
    const before = conn.frames.length;
    for (const json of [`[${STATUS}]`, '"text"', "{", '{"jsonrpc":"2.0","method":"initialize"}']) {
      port.send(json);
    }
    await probe(host, port, got);
    expect(conn.frames).toHaveLength(before + 1);
    expect(got.messages).toHaveLength(1);
    port.close();
  });

  test("sendBlob sends one 0x02 frame with the header", async () => {
    const { port, conn } = await opened();
    const chunk = { hash: "ab".repeat(32), offset: 5, last: true, bytes: new Uint8Array([1, 2]) };
    port.sendBlob(chunk.hash, chunk.offset, chunk.last, chunk.bytes.slice().buffer);
    await until(() => expect(conn.frames.at(-1)?.type).toBe(FRAME_BLOB));
    expect(decodeBlobChunk(conn.frames.at(-1)?.payload ?? new Uint8Array())).toEqual(chunk);
    port.close();
  });

  test("each chunk is its own ArrayBuffer, and an unsubscribed callback gets no more", async () => {
    const { host, port } = await opened();
    const kept = inbound(port);
    const dropped = inbound(port);
    host.notify("host/shutdown", SHUTDOWN);
    await until(() => expect(dropped.messages).toHaveLength(1));
    dropped.off();
    host.notify("host/shutdown", SHUTDOWN);
    // Both callbacks get a chunk at once: once `kept` has it, `dropped` would.
    await until(() => expect(kept.messages).toHaveLength(2));
    expect(dropped.messages).toHaveLength(1);
    const types = kept.chunks.map((chunk) => Object.prototype.toString.call(chunk));
    expect(new Set(types)).toEqual(new Set(["[object ArrayBuffer]"]));
    port.ack((kept.chunks as ArrayBuffer[]).reduce((n, chunk) => n + chunk.byteLength, 0));
    port.close();
  });

  test("a reconnect delivers the old data, then reconnecting, then ready with a handshake", async () => {
    const { shell, host, port, conn } = await opened();
    const timeline: unknown[] = [];
    inbound(port, timeline);
    shell.onConnectionState((state, handshake) => timeline.push([state.status, handshake?.host]));
    conn.send({ jsonrpc: "2.0", method: "host/shutdown", params: SHUTDOWN });
    conn.close();
    const reconnected = [
      { method: "host/shutdown" },
      ["reconnecting", undefined],
      ["ready", host.init.host],
    ];
    await until(() => expect(timeline).toMatchObject(reconnected));
    port.send(STATUS);
    const sent = () => host.connections.at(-1)?.messages().at(-1);
    await until(() => expect(sent()).toMatchObject({ id: 7, method: "host/status" }));
    expect(host.connections.at(-1)).not.toBe(conn);
    port.close();
  });

  test("close ends the connection for good", async () => {
    const { shell, host, port, conn } = await opened();
    const states: unknown[] = [];
    shell.onConnectionState((state) => states.push(state));
    port.close();
    await until(() => expect(conn.closed).toBe(true));
    port.send(STATUS);
    port.sendBlob("ab".repeat(32), 0, true, new ArrayBuffer(1));
    // A closed port answers nothing, so there is no event to wait for: the
    // pause gives a wrong send or reconnect the time an idle machine needs
    // to show it. A loaded one may hide it, never fail the case.
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(host.connections.filter((c) => !c.closed)).toEqual([]);
    expect(states).toEqual([]);
  });
}
