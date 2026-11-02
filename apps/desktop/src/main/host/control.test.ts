// UNIT-launcher, the control connection: one connection at a time (T214).
// The launcher and the sockets are fakes.

import { EventEmitter } from "node:events";

import { describe, expect, test } from "vitest";

import type { MainLog } from "../log";
import type { ConnectOptions } from "./connection";
import { ControlConnection } from "./control";
import type { HostLauncher } from "./launcher";

function control() {
  const state = { sockets: [] as EventEmitter[] };
  const port = { send: () => undefined, onData: () => () => undefined, close: () => undefined };
  const open = () => {
    const socket = new EventEmitter();
    state.sockets.push(socket);
    return Promise.resolve({ port, handshake: {}, rest: new Uint8Array(), socket });
  };
  const launcher = { open } as unknown as HostLauncher;
  const log = { warn: () => undefined } as unknown as MainLog;
  return { conn: new ControlConnection(launcher, {} as ConnectOptions, log), state };
}

describe("UNIT-launcher", () => {
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
