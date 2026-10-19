// The browser mock of `ShellAPI.host` (design 5.7.3): it plays the main
// process in front of a MockHost, the way the windowBridge (T076) does in
// front of the real host, and passes the same contract tests (`../contract`).

import type {
  ConnectionState,
  Handshake,
  HostPort,
  ShellAPI,
  Unsubscribe,
} from "@cupper/shell-api";

import { encodeBlobChunk, encodeJson } from "../frame";
import { handshake, HandshakeError, type HandshakeOptions } from "../handshake";
import { Listeners } from "../listeners";
import { hostError, type MockConnection, type MockHost, type Reply } from "./host";

/** What the mock main process says in `initialize`; the mock host takes any proof. */
export const MOCK_HANDSHAKE: HandshakeOptions = {
  client: { kind: "desktop", name: "Cupper", version: "0.0.0" },
  role: "window",
  features: ["stream.coalesce", "blob.get", "blob.put", "history.page"],
  auth: () => ({ type: "dev_key", c_nonce: "b".repeat(64), proof: "c".repeat(64) }),
  verifyHostProof: () => true,
  refuseWeakAuth: false,
};

/** One window's port. Like a MessagePort, it delivers later and in order, and keeps what comes before a listener. */
class WindowPort implements HostPort {
  closed = false;
  #conn: MockConnection | undefined;
  #offConn: Unsubscribe | undefined;
  readonly #data = new Listeners<[ArrayBuffer]>();
  #backlog: ArrayBuffer[] | undefined = [];

  post(task: () => void): void {
    queueMicrotask(() => {
      if (!this.closed) {
        task();
      }
    });
  }

  use(conn: MockConnection, rest: Uint8Array): void {
    this.#offConn?.();
    this.#conn = conn;
    if (rest.length > 0) {
      this.#deliver(rest);
    }
    this.#offConn = conn.onData((bytes) => this.#deliver(bytes));
  }

  send(json: string): void {
    let message: unknown;
    try {
      message = JSON.parse(json);
    } catch {
      return;
    }
    if (this.closed || typeof message !== "object" || message === null || Array.isArray(message)) {
      return;
    }
    const answer = (reply: Reply) => {
      if ("id" in message) {
        this.#deliver(encodeJson({ jsonrpc: "2.0", id: message.id, ...reply }));
      }
    };
    if ("method" in message && message.method === "initialize") {
      answer(hostError("forbidden", "already_initialized"));
      return;
    }
    try {
      this.#conn?.write(encodeJson(message));
    } catch {
      answer(hostError("too_large"));
    }
  }

  sendBlob(hash: string, offset: number, last: boolean, bytes: ArrayBuffer): void {
    try {
      if (!this.closed) {
        this.#conn?.write(encodeBlobChunk({ hash, offset, last, bytes: new Uint8Array(bytes) }));
      }
    } catch {
      // A malformed header is not forwarded; the `blob/put` that follows fails.
    }
  }

  onData(cb: (chunk: ArrayBuffer) => void): Unsubscribe {
    const off = this.#data.add(cb);
    const backlog = this.#backlog ?? [];
    this.#backlog = undefined;
    backlog.forEach((chunk) => this.post(() => this.#data.emit(chunk)));
    return off;
  }

  /** Bytes the window handled; the mock has no flow control to apply them to. */
  acked = 0;

  ack(bytes: number): void {
    this.acked += bytes;
  }

  close(): void {
    this.closed = true;
    this.#offConn?.();
    this.#conn?.close();
  }

  #deliver(bytes: Uint8Array): void {
    const chunk = bytes.slice().buffer;
    if (this.#backlog) {
      this.#backlog.push(chunk);
    } else {
      this.post(() => this.#data.emit(chunk));
    }
  }
}

/** `ShellAPI.host` in front of `host`. A connection the host drops is made again at once. */
export function createMockShellHost(host: MockHost): ShellAPI["host"] {
  const states = new Listeners<[ConnectionState, Handshake | undefined]>();
  let current: WindowPort | undefined;

  async function connect(port: WindowPort) {
    const conn = host.connect();
    const done = await handshake(conn.port(), MOCK_HANDSHAKE);
    conn.onClose(() => {
      if (!port.closed) {
        port.post(() => states.emit({ status: "reconnecting" }, undefined));
        void reconnect(port);
      }
    });
    return { conn, ...done };
  }

  async function reconnect(port: WindowPort) {
    try {
      const { conn, handshake, rest } = await connect(port);
      port.post(() => states.emit({ status: "ready" }, handshake));
      port.use(conn, rest);
    } catch (err) {
      const reason = err instanceof HandshakeError ? err.reason : "protocol_error";
      port.post(() => states.emit({ status: "failed", reason }, undefined));
    }
  }

  return {
    open: async () => {
      current?.close();
      const port = (current = new WindowPort());
      const { conn, handshake, rest } = await connect(port);
      port.use(conn, rest);
      return { port, handshake };
    },
    onConnectionState: (cb) => states.add(cb),
  };
}
