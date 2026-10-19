import type { HostPort } from "@cupper/shell-api";
import { describe, expect, test } from "vitest";

import { ConnectionLost, HostClient, RpcFailure, uuidv7 } from "./client";
import type { BlobChunk } from "./frame";
import { handshake } from "./handshake";
import { hostError, MockHost, type HostRequest, type MockConnection } from "./mock/host";
import { createMockShellHost, MOCK_HANDSHAKE } from "./mock/port";
import { connectShell } from "./shell";
import { fixtures } from "./testing";

const quiet = () => new HostClient({ log: () => undefined });

/** `client` on a new connection of `host`, through `wrap` (nothing in between by default). */
async function attach(host: MockHost, client: HostClient, wrap = (port: HostPort) => port) {
  const conn = host.connect();
  const { handshake: done, rest } = await handshake(conn.port(), MOCK_HANDSHAKE);
  client.attach(wrap(conn.port()), done, rest);
  return conn;
}

describe("UNIT-hc-conn", () => {
  test("state machine: connecting, handshaking, ready, reconnecting, failed, closed", async () => {
    const host = new MockHost(fixtures);
    const client = quiet();
    const seen: string[] = [];
    client.onState((state, done) => seen.push(`${state.status}${done ? " +" : ""}`));
    client.detach();
    client.progress("handshaking");
    await attach(host, client);
    client.detach();
    client.progress("connecting");
    client.progress("handshaking");
    const conn = await attach(host, client);
    client.fail("host_unverified");
    client.close();
    client.attach(conn.port(), host.init);
    expect(seen).toEqual(["handshaking", "ready +", "reconnecting", "ready +", "failed", "closed"]);
    expect([client.state, conn.closed]).toEqual([{ status: "closed" }, true]);
  });

  test("responses find their requests in any order; notifications, chunks and acks", async () => {
    const host = new MockHost(fixtures);
    const waiting: HostRequest[] = [];
    host.handle("host/status", (request) => void waiting.push(request));
    host.handle("session/get", () => hostError("not_found"));
    const logs: string[] = [];
    const client = new HostClient({ log: (message) => logs.push(message) });
    let [received, acked] = [0, 0];
    const conn = await attach(host, client, (port) => ({
      ...port,
      onData: (cb) =>
        port.onData((chunk) => {
          received += chunk.byteLength;
          cb(chunk);
        }),
      ack: (bytes) => (acked += bytes),
    }));
    const seen: unknown[] = [];
    client.onNotification((method, params) => seen.push([method, params]));
    client.onBlobChunk((chunk) => seen.push(chunk));

    const [first, second] = [client.request("host/status"), client.request("host/status")];
    const missing = client.request("session/get", { session_id: "s_9" });
    waiting[1]?.conn.reply(waiting[1].id, { result: 2 });
    waiting[0]?.conn.reply(waiting[0].id, { result: 1 });
    conn.send({ jsonrpc: "2.0", id: 99, result: {} });
    conn.send({ jsonrpc: "2.0", method: "ui/navigate", params: { session_id: "s_1" } });
    const chunk: BlobChunk = {
      hash: "ab".repeat(32),
      offset: 3,
      last: true,
      bytes: new Uint8Array([7]),
    };
    conn.sendBlob(chunk);

    expect([await first, await second]).toEqual([1, 2]);
    await expect(missing).rejects.toMatchObject({ error: { data: { code: "not_found" } } });
    await expect(missing).rejects.toBeInstanceOf(RpcFailure);
    expect(seen).toEqual([["ui/navigate", { session_id: "s_1" }], chunk]);
    expect(logs).toEqual(["skipped a message that answers nothing sent: 99"]);
    expect(acked).toBe(received);
  });

  test("a command goes again with the same command_id after a reconnect; a request fails", async () => {
    const host = new MockHost(fixtures);
    host.handle("host/status", () => undefined);
    let drops = 1;
    host.handle("session/send", ({ conn }) => {
      // The connection goes before the host answers.
      return drops-- > 0 ? void conn.close() : { result: { disposition: "queued" } };
    });
    const client = new HostClient({ commandId: () => "c-1", log: () => undefined });
    await connectShell(createMockShellHost(host), client);
    const status = client.request("host/status");
    const sent = client.command("session/send", { session_id: "s_1", content: [] });
    await expect(status).rejects.toBeInstanceOf(ConnectionLost);
    expect(await sent).toEqual({ disposition: "queued" });
    const sends = host.requests.filter((r) => r.method === "session/send");
    const params = { session_id: "s_1", content: [], command_id: "c-1" };
    expect(sends.map((r) => r.params)).toEqual([params, params]);
    expect(sends.map((r) => host.connections.indexOf(r.conn))).toEqual([0, 1]);
  });

  test("without a connection requests fail at once, commands wait, and close ends both", async () => {
    const host = new MockHost(fixtures);
    host.handle("session/rename", () => undefined);
    host.handle("session/interrupt", () => ({ result: "interrupted" }));
    const client = quiet();
    await expect(client.request("ping")).rejects.toBeInstanceOf(ConnectionLost);
    const queued = client.command("session/interrupt", { session_id: "s_1", command_id: "c-9" });
    const conn = await attach(host, client);
    expect(await queued).toBe("interrupted");
    expect(host.requests.at(-1)?.params).toEqual({ session_id: "s_1", command_id: "c-9" });
    const pending = client.command("session/rename", { session_id: "s_1", name: "x" });
    client.detach();
    const later = client.command("session/queue_resume", { session_id: "s_1" });
    client.close();
    await expect(pending).rejects.toBeInstanceOf(ConnectionLost);
    await expect(later).rejects.toBeInstanceOf(ConnectionLost);
    expect(conn.messages().at(-1)).toMatchObject({ method: "session/rename" });
  });

  test("a broken frame closes the port and fails with protocol_error", async () => {
    const host = new MockHost(fixtures);
    const client = quiet();
    let feed: ((chunk: ArrayBuffer) => void) | undefined;
    const conn: MockConnection = await attach(host, client, (port) => ({
      ...port,
      onData: (cb) => {
        feed = cb;
        return () => undefined;
      },
    }));
    feed?.(new Uint8Array([0, 0, 0, 0]).buffer);
    expect([client.state, conn.closed]).toEqual([
      { status: "failed", reason: "protocol_error" },
      true,
    ]);
  });

  test("command IDs are UUIDv7", () => {
    const id = uuidv7(0x0190a3b4c5d6, new Uint8Array(10).fill(0xff));
    expect(id).toBe("0190a3b4-c5d6-7fff-bfff-ffffffffffff");
    expect(uuidv7()).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
  });
});
