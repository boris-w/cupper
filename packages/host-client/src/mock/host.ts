// A frame-level mock of the host: it speaks the wire format of design 5.3.1
// over in-memory byte connections, for the host-client's tests, the browser
// mock HostPort and the windowBridge tests (T076). Requests are answered from
// `protocol/fixtures/methods.json` unless a test sets a handler.

import type { HostHelloParams, InitializeResult, RpcError } from "@cupper/protocol";
import type { HostPort, Unsubscribe } from "@cupper/shell-api";

import {
  encodeBlobChunk,
  encodeFrame,
  encodeJson,
  FRAME_JSON,
  FrameDecoder,
  type BlobChunk,
  type Frame,
} from "../frame";
import { Listeners } from "../listeners";

/** One case of `protocol/fixtures/methods.json`. */
export interface MethodFixture {
  method: string;
  params: unknown;
  result?: unknown;
}

export type Reply = { result: unknown } | { error: RpcError };

export interface HostRequest {
  id: unknown;
  method: string;
  params: unknown;
  conn: MockConnection;
}

/** Answers a request; `undefined` leaves it unanswered, for `conn.reply` later. */
export type Handler = (request: HostRequest) => Reply | undefined;

/** An application error (design 5.3.8). */
export function hostError(code: string, reason?: string): { error: RpcError } {
  const message = reason === undefined ? code : `${code}/${reason}`;
  return { error: { code: -32000, message, data: { code, reason } } };
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** One connection: the client writes bytes in, the host's frames come out. */
export class MockConnection {
  /** Every frame the client wrote, in order. */
  readonly frames: Frame[] = [];
  initialized = false;
  readonly #host: MockHost;
  readonly #decoder = new FrameDecoder();
  readonly #data = new Listeners<[Uint8Array]>();
  readonly #closes = new Listeners<[]>();
  /** What the host sent before the client listened. */
  #backlog: Uint8Array[] | undefined = [];
  #closed = false;

  constructor(host: MockHost) {
    this.#host = host;
  }

  get closed(): boolean {
    return this.#closed;
  }

  /** Bytes from the client. */
  write(bytes: Uint8Array): void {
    if (this.#closed) {
      return;
    }
    this.#decoder.push(bytes);
    for (let frame = this.#decoder.next(); frame; frame = this.#decoder.next()) {
      this.frames.push(frame);
      if (frame.type === FRAME_JSON) {
        this.#host.receive(this, parse(frame.payload));
      }
    }
  }

  /** Bytes from the host, one call per frame. The first listener also gets what came before it. */
  onData(cb: (bytes: Uint8Array) => void): Unsubscribe {
    const off = this.#data.add(cb);
    const backlog = this.#backlog ?? [];
    this.#backlog = undefined;
    backlog.forEach((bytes) => this.#data.emit(bytes));
    return off;
  }

  onClose(cb: () => void): Unsubscribe {
    return this.#closes.add(cb);
  }

  /** Either end hangs up. */
  close(): void {
    if (!this.#closed) {
      this.#closed = true;
      this.#closes.emit();
    }
  }

  /** The JSON messages the client sent, in order. */
  messages(): unknown[] {
    return this.frames.filter((f) => f.type === FRAME_JSON).map((f) => parse(f.payload));
  }

  send(message: unknown): void {
    this.#emit(encodeJson(message));
  }

  sendBlob(chunk: BlobChunk): void {
    this.#emit(encodeBlobChunk(chunk));
  }

  reply(id: unknown, reply: Reply): void {
    this.send({ jsonrpc: "2.0", id, ...reply });
  }

  /** This connection as a HostPort with nothing in between: no checks, no delay. */
  port(): HostPort {
    return {
      send: (json) => this.write(encodeFrame(FRAME_JSON, new TextEncoder().encode(json))),
      sendBlob: (hash, offset, last, bytes) =>
        this.write(encodeBlobChunk({ hash, offset, last, bytes: new Uint8Array(bytes) })),
      onData: (cb) => this.onData((bytes) => cb(bytes.slice().buffer)),
      ack: () => undefined,
      close: () => this.close(),
    };
  }

  #emit(bytes: Uint8Array): void {
    if (this.#closed) {
      return;
    }
    if (this.#backlog) {
      this.#backlog.push(bytes);
    } else {
      this.#data.emit(bytes);
    }
  }
}

function parse(payload: Uint8Array): unknown {
  try {
    return JSON.parse(new TextDecoder().decode(payload));
  } catch {
    return undefined;
  }
}

export class MockHost {
  /** Every connection, in the order they were made. */
  readonly connections: MockConnection[] = [];
  /** Every request and client notification, in order. */
  readonly requests: HostRequest[] = [];
  /** What `host/hello` says on the next connection. */
  hello: HostHelloParams = {
    nonce: "a".repeat(64),
    protocol: { min: 1, max: 1 },
    host_version: "0.1.0",
  };
  /** The result of `initialize`: the first fixture's, which is a desktop window's. */
  init: InitializeResult;
  readonly #results = new Map<string, unknown>();
  readonly #handlers = new Map<string, Handler>();

  constructor(fixtures: readonly MethodFixture[]) {
    for (const { method, result } of fixtures) {
      if (result !== undefined && !this.#results.has(method)) {
        this.#results.set(method, result);
      }
    }
    const init = this.#results.get("initialize");
    if (init === undefined) {
      throw new Error("the fixtures have no initialize result");
    }
    this.init = structuredClone(init) as InitializeResult;
  }

  /** Answers `method` with `handler` instead of the fixture. */
  handle(method: string, handler: Handler): void {
    this.#handlers.set(method, handler);
  }

  /** A new connection; `host/hello` is on its way at once. */
  connect(): MockConnection {
    const conn = new MockConnection(this);
    this.connections.push(conn);
    conn.send({ jsonrpc: "2.0", method: "host/hello", params: this.hello });
    return conn;
  }

  /** A notification to every open connection past its handshake. */
  notify(method: string, params: unknown): void {
    for (const conn of this.connections) {
      if (conn.initialized && !conn.closed) {
        conn.send({ jsonrpc: "2.0", method, params });
      }
    }
  }

  /** A message from `conn`'s client. */
  receive(conn: MockConnection, message: unknown): void {
    if (!isRecord(message) || typeof message.method !== "string") {
      return;
    }
    const request = { id: message.id, method: message.method, params: message.params, conn };
    this.requests.push(request);
    if (!("id" in message)) {
      return;
    }
    const reply = this.#answer(request);
    if (reply) {
      conn.reply(request.id, reply);
    }
    // A failed handshake, or a second one, ends the connection (T030).
    if (request.method === "initialize" && reply && "error" in reply) {
      conn.close();
    }
  }

  #answer(request: HostRequest): Reply | undefined {
    const { conn, method, params } = request;
    const handler = this.#handlers.get(method);
    if (method === "initialize") {
      if (conn.initialized) {
        return hostError("forbidden", "already_initialized");
      }
      const nonce = isRecord(params) ? params.nonce : undefined;
      const reply = handler
        ? handler(request)
        : nonce === this.hello.nonce
          ? { result: this.init }
          : hostError("unauthenticated", "bad_nonce");
      conn.initialized ||= reply !== undefined && "result" in reply;
      return reply;
    }
    if (!conn.initialized) {
      return hostError("not_initialized");
    }
    if (handler) {
      return handler(request);
    }
    const result = this.#results.get(method);
    return result === undefined
      ? { error: { code: -32601, message: `no method ${method}` } }
      : { result };
  }
}
