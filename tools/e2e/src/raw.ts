// The host protocol by hand, for the connection-level security tests (design
// 5.3.1, 5.8.1): a raw client that frames messages itself
// (`u32 length | u8 type | payload`) and offers in `initialize` whatever
// `auth` a test gives it, and a fake host that holds the socket first, greets
// like the host and answers `initialize` with a `host_proof` it cannot know.
// Both record what the other side sent. Test code only.

import { createHmac, randomBytes } from "node:crypto";
import { mkdirSync, readFileSync } from "node:fs";
import { connect, createServer, type Server, type Socket } from "node:net";
import { dirname, join } from "node:path";

export type Message = Record<string, unknown>;

/** `<data dir>/run/host.sock`: the e2e roots are short enough for it (design 5.3.1). */
export const socketPath = (dataDir: string) => join(dataDir, "run", "host.sock");

/** One end of a connection: the JSON messages it got, as they come. */
export class Peer {
  readonly messages: Message[] = [];
  readonly closed: Promise<void>;
  #buffer = Buffer.alloc(0);
  #wake: () => void = () => undefined;

  constructor(readonly socket: Socket) {
    socket.on("data", (chunk) => {
      this.#buffer = Buffer.concat([this.#buffer, chunk]);
      const end = () => (this.#buffer.length < 4 ? Infinity : 4 + this.#buffer.readUInt32BE(0));
      while (this.#buffer.length >= end()) {
        const payload = this.#buffer.subarray(5, end());
        if (this.#buffer[4] === 0x01) this.messages.push(JSON.parse(payload.toString()) as Message);
        this.#buffer = this.#buffer.subarray(end());
      }
      this.#wake();
    });
    socket.on("error", () => undefined);
    this.closed = new Promise((done) => socket.once("close", () => done()));
    void this.closed.then(() => this.#wake());
  }

  /** Writes one frame: a JSON message, or `0x02` with a blob chunk's payload. */
  send(message: Message | Buffer, type = 0x01): void {
    const payload = Buffer.isBuffer(message) ? message : Buffer.from(JSON.stringify(message));
    const head = Buffer.alloc(5);
    head.writeUInt32BE(payload.length + 1);
    head[4] = type;
    this.socket.write(Buffer.concat([head, payload]));
  }

  /** The first message that `test` accepts; fails once the connection closed without one. */
  async next(test: (message: Message) => boolean, ms = 10_000): Promise<Message> {
    const deadline = Date.now() + ms;
    for (;;) {
      const found = this.messages.find(test);
      if (found) return found;
      if (this.socket.closed || Date.now() > deadline) {
        throw new Error(`no such message; got ${JSON.stringify(this.messages)}`);
      }
      await new Promise<void>((wake) => {
        this.#wake = wake;
        setTimeout(wake, 100);
      });
    }
  }

  /** A request with `id`, and its response. */
  async request(id: number, method: string, params: unknown): Promise<Message> {
    this.send({ jsonrpc: "2.0", id, method, params });
    return this.next((message) => message.id === id && !("method" in message));
  }
}

/** A client on the host's socket, after `host/hello`; its nonce is `nonce`. */
export async function dial(dataDir: string): Promise<Peer & { nonce: string }> {
  const socket = connect(socketPath(dataDir));
  const peer = new Peer(socket);
  const hello = await peer.next((message) => message.method === "host/hello");
  return Object.assign(peer, { nonce: (hello.params as { nonce: string }).nonce });
}

/** `initialize` offering `auth`, and more params; the response. */
export function initialize(peer: Peer & { nonce: string }, auth: Message, more: Message = {}) {
  const client = { kind: "cli", name: "raw", version: "0.0.0" };
  const protocol = { min: 1, max: 1 };
  const params = { protocol, nonce: peer.nonce, client, auth, features: [], ...more };
  return peer.request(1, "initialize", params);
}

/** A key proof of this process (design 5.8.1): HMAC-SHA256 over "c2h", both nonces, our pid. */
export function keyProof(type: "dev_key" | "harness", key: Buffer, hNonce: string): Message {
  const cNonce = randomBytes(32);
  const pid = Buffer.alloc(4);
  pid.writeUInt32BE(process.pid);
  const signed = Buffer.concat([Buffer.from("c2h"), Buffer.from(hNonce, "hex"), cNonce, pid]);
  const proof = createHmac("sha256", key).update(signed).digest("hex");
  return { type, c_nonce: cNonce.toString("hex"), proof };
}

const FIXTURES = new URL("../../../protocol/fixtures/methods.json", import.meta.url);
/** A well-formed `initialize` result: the protocol's fixture. */
const RESULT = (JSON.parse(readFileSync(FIXTURES, "utf8")) as Message[]).find(
  (fixture) => fixture.method === "initialize",
)?.result as Message;

/** A fake host on the socket of `dataDir`. */
export class FakeHost {
  readonly peers: Peer[] = [];
  #server: Server | undefined;

  async listen(dataDir: string): Promise<this> {
    const path = socketPath(dataDir);
    mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
    this.#server = createServer((socket) => {
      const peer = new Peer(socket);
      this.peers.push(peer);
      const nonce = randomBytes(32).toString("hex");
      const params = { nonce, protocol: { min: 1, max: 1 }, host_version: "0.1.0" };
      peer.send({ jsonrpc: "2.0", method: "host/hello", params });
      const proof = randomBytes(32).toString("hex");
      void peer
        .next((message) => message.method === "initialize")
        .then(
          ({ id }) => peer.send({ jsonrpc: "2.0", id, result: { ...RESULT, host_proof: proof } }),
          () => undefined,
        );
    });
    await new Promise<void>((done) => this.#server?.listen(path, done));
    return this;
  }

  /** What the clients sent, every connection's in turn. */
  received(): Message[] {
    return this.peers.flatMap((peer) => peer.messages);
  }

  async close(): Promise<void> {
    this.peers.forEach((peer) => peer.socket.destroy());
    await new Promise((done) => this.#server?.close(done));
  }
}
