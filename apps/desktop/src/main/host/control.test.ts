// UNIT-launcher, the control connection: one connection at a time, and none
// once quitting is decided (T214). The launcher and the sockets are fakes.

import { EventEmitter } from "node:events";

import { describe, expect, test } from "vitest";

import type { MainLog } from "../log";
import type { ConnectOptions } from "./connection";
import { ControlConnection } from "./control";
import type { HostLauncher } from "./launcher";

function control(opened: Promise<unknown> = Promise.resolve()) {
  const state = { closed: false, sockets: [] as EventEmitter[], conns: 0 };
  const port = { send: () => undefined, onData: () => () => undefined, close: () => undefined };
  const conn = { close: () => void (state.conns -= 1) };
  const open = async () => {
    await opened;
    const socket = new EventEmitter();
    state.sockets.push(socket);
    state.conns += 1;
    return { port, handshake: {}, rest: new Uint8Array(), socket, conn };
  };
  const launcher = { open, close: () => void (state.closed = true) } as unknown as HostLauncher;
  const log = { warn: () => undefined } as unknown as MainLog;
  return { conn: new ControlConnection(launcher, {} as ConnectOptions, log), state };
}

describe("UNIT-launcher", () => {
  test("once quitting is decided, a dropped control connection is not made again", async () => {
    const { conn, state } = control();
    await conn.connect();
    const exit = conn.client.command("host/exit", { force: true });
    conn.end();
    state.sockets[0]?.emit("close");
    expect(state).toMatchObject({ sockets: { length: 1 }, closed: true });
    expect(conn.client.state).toEqual({ status: "closed" });
    // `host/exit` is not sent again, nor waited for.
    await expect(exit).rejects.toThrow();
  });

  test("a connection that comes after quitting is decided is closed, not taken (T225)", async () => {
    const { promise, resolve } = Promise.withResolvers<undefined>();
    const { conn, state } = control(promise);
    const connecting = conn.connect();
    conn.end();
    resolve(undefined);
    await connecting;
    expect(state).toMatchObject({ sockets: { length: 1 }, conns: 0 });
    expect(conn.client.state.status).toBe("connecting");
  });

  test("connecting while a connection is up or being made opens no other", async () => {
    // As restarting the host does, maybe while a dropped one connects again.
    const { conn, state } = control();
    await Promise.all([conn.connect(), conn.connect()]);
    await conn.connect();
    expect(state.sockets).toHaveLength(1);
    state.sockets[0]?.emit("close");
    await conn.connect();
    expect(state.sockets).toHaveLength(2);
    expect(conn.client.state.status).toBe("ready");
  });
});
