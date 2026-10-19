// UNIT-native: this Node process loads the `dev-auth` build of the extension,
// starts the gateway fixture (an ad-hoc signed test-harness build) with it,
// and connects as the desktop does. `pnpm run build:native` builds both.

import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtempSync, rmSync } from "node:fs";
import net from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";

import { afterEach, describe, expect, test } from "vitest";

import type { DevAuthNative, HostProcess, KeyedHostConnection } from "./index.ts";

const TARGET = process.env["CARGO_TARGET_DIR"] ?? resolve(import.meta.dirname, "../../../target");
const FIXTURE = join(TARGET, "debug/cupper-gateway-fixture");

function load(): DevAuthNative {
  const module = { exports: {} };
  process.dlopen(module, join(TARGET, "debug/libcupper_native.dylib"));
  return module.exports as DevAuthNative;
}

const native = load();

interface Message {
  id?: number;
  method?: string;
  params?: { nonce?: string };
  result?: { host_proof?: string; principal?: { id?: string } };
  error?: unknown;
}

/** JSON frames (`u32 length | u8 type | payload`) on the main process's end. */
class Wire {
  readonly socket: net.Socket;
  readonly closed: Promise<unknown>;
  received = 0;
  private buffer = Buffer.alloc(0);
  private readonly messages: Message[] = [];

  constructor(fd: number) {
    this.socket = new net.Socket({ fd, readable: true, writable: true });
    this.closed = new Promise((resolve) => this.socket.on("close", resolve));
    this.socket.on("data", (chunk: Buffer) => {
      this.received += chunk.length;
      this.buffer = Buffer.concat([this.buffer, chunk]);
      while (this.buffer.length >= 4 && this.buffer.length >= 4 + this.buffer.readUInt32BE(0)) {
        const end = 4 + this.buffer.readUInt32BE(0);
        this.messages.push(JSON.parse(this.buffer.subarray(5, end).toString()) as Message);
        this.buffer = this.buffer.subarray(end);
      }
    });
  }

  send(message: object): void {
    const payload = Buffer.from(JSON.stringify({ jsonrpc: "2.0", ...message }));
    const header = Buffer.alloc(5);
    header.writeUInt32BE(payload.length + 1);
    header[4] = 0x01;
    this.socket.write(Buffer.concat([header, payload]));
  }

  async next(): Promise<Message> {
    for (;;) {
      const message = this.messages.shift();
      if (message) return message;
      const ended = await Promise.race([once(this.socket, "data"), this.closed.then(() => true)]);
      if (ended === true) throw new Error("the connection closed");
    }
  }
}

const cleanup: (() => void)[] = [];
afterEach(() => cleanup.splice(0).forEach((step) => step()));

function tempDir(): string {
  const dir = mkdtempSync(join(tmpdir(), "cupper-native-"));
  cleanup.push(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}

/** The fixture, started with `key`, and the socket it listens on. */
async function startFixture(key: InstanceType<DevAuthNative["HostKey"]>) {
  const dir = tempDir();
  const command = { program: FIXTURE, args: ["--data-dir", dir], env: {}, pipeStdout: true };
  const host: HostProcess = native.spawnHost(command, key);
  let running = true;
  void host.exit.then(() => (running = false));
  cleanup.push(() => running && process.kill(host.pid));
  if (host.stdout === undefined) throw new Error("no pipe on the fixture's stdout");
  const stdout = new net.Socket({ fd: host.stdout, readable: true, writable: false });
  const [line] = (await once(createInterface({ input: stdout }), "line")) as [string];
  stdout.destroy();
  const { socket } = JSON.parse(line) as { socket: string };
  expect(socket).toBe(native.socketPath(dir));
  return { host, socket };
}

/** `host/hello`, then `initialize` of the control connection with `auth`. */
async function initialize(conn: KeyedHostConnection, wire: Wire): Promise<Message> {
  const hello = await wire.next();
  expect(hello.method).toBe("host/hello");
  const nonce = hello.params?.nonce ?? "";
  const auth = conn.auth(nonce);
  expect(() => conn.auth(nonce)).toThrow(
    expect.objectContaining({ code: "already_authenticated" }),
  );
  const client = { kind: "desktop", name: "UNIT-native", version: "0" };
  const params = {
    protocol: { min: 1, max: 1 },
    nonce,
    client,
    role: "control",
    auth,
    features: [],
  };
  wire.send({ id: 1, method: "initialize", params });
  return wire.next();
}

describe("UNIT-native", () => {
  test("starts the fixture, verifies it, and the key is proven both ways", async () => {
    expect(native.buildInfo()).toEqual({ features: ["dev-auth"], hostCheck: "key_proof" });
    const key = new native.HostKey();
    const { host, socket } = await startFixture(key);
    const conn = native.connectHost(socket, key);
    expect(conn.hostPid).toBe(host.pid);
    const wire = new Wire(conn.takeFd());
    expect(() => conn.takeFd()).toThrow(expect.objectContaining({ code: "fd_taken" }));

    const response = await initialize(conn, wire);
    expect(response.result?.principal?.id).toBe("pr_desktop");
    expect(conn.verifyHostProof(response.result?.host_proof)).toBe(true);
    wire.send({ id: 2, method: "ping" });
    expect(await wire.next()).toMatchObject({ id: 2, result: {} });
    expect(conn.closeReason).toBeNull();

    conn.close();
    await wire.closed;
    expect(conn.closeReason).toBe("local");
    process.kill(host.pid, "SIGTERM");
    expect(await host.exit).toEqual({ signal: 15 });
  });

  test("a wrong or missing host_proof disconnects", async () => {
    const key = new native.HostKey();
    const { socket } = await startFixture(key);
    for (const hostProof of ["0".repeat(64), undefined]) {
      const conn = native.connectHost(socket, key);
      const wire = new Wire(conn.takeFd());
      const response = await initialize(conn, wire);
      expect(response.result?.host_proof).toMatch(/^[0-9a-f]{64}$/);
      expect(conn.verifyHostProof(hostProof)).toBe(false);
      await wire.closed;
      expect(conn.closeReason).toBe("host_unverified");
    }
  });

  test("another process speaking on the host's end disconnects", async () => {
    // A fake host in this process; `sh` writes on its end of the connection.
    const path = join(tempDir(), "host.sock");
    const server = net.createServer();
    cleanup.push(() => server.close());
    const accepted = once(server, "connection") as Promise<[net.Socket]>;
    server.listen(path);
    await once(server, "listening");
    const conn = native.connectHost(path, new native.HostKey());
    const wire = new Wire(conn.takeFd());
    const [hostEnd] = await accepted;
    const sh = spawn("/bin/sh", ["-c", "printf x"], { stdio: ["ignore", hostEnd, "ignore"] });
    await once(sh, "exit");
    await wire.closed;
    expect(wire.received).toBe(0);
    expect(conn.closeReason).toBe("token_changed");
    hostEnd.destroy();
  });

  test("refuses what the payload and the environment must not carry", () => {
    const key = new native.HostKey();
    const command = { program: FIXTURE, args: [], env: {} };
    const payload = { dev_key: "0".repeat(64) };
    const invalid = expect.objectContaining({ code: "invalid_argument" });
    expect(() => native.spawnHost(command, key, payload)).toThrow(invalid);
    expect(() => native.spawnHost({ ...command, env: { CUPPER_X: "1" } }, key)).toThrow(invalid);
    const missing = join(tempDir(), "none.sock");
    const failed = expect.objectContaining({ code: "connect_failed" });
    expect(() => native.connectHost(missing, key)).toThrow(failed);
  });
});
