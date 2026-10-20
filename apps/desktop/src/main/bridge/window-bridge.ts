// windowBridge (design 5.7.1, 5.7.2): one per document instance, between the
// window's MessagePort and its data connection to the host (role `window`).
//
// - Inbound bytes are not parsed: they gather for 16 ms and go to the window
//   as one ArrayBuffer; the host-client cuts the frames.
// - Outbound, the window's messages are parsed, checked against the command
//   allowlist (design 5.7.8) and sent re-serialized, never as the bytes that
//   came in. A refused request gets a `forbidden` error response through the
//   data, `initialize` a `forbidden/already_initialized`; a refused message
//   without an `id`, a batch and anything that is not a JSON object are
//   dropped. Integers beyond ±(2^53 − 1) would change in the parse, so a
//   message with one is refused too.
// - Real input (design 5.7.8, rules 4 and 9): every command uses up the
//   window's input not used yet; `interaction/respond` and `git/revert` need
//   it, at most 1 s old, and carry it as `input`, over whatever the window
//   wrote. Without it they are refused here, `user_activation_required`,
//   unless they repeat, with the same parameters, a command the host has
//   not answered: the connection dropped before it could.
// - Flow control: past 8 MB the window has not acknowledged, the bridge
//   stops reading the connection until it does.
// - When the connection drops, everything it brought reaches the window, then
//   `reconnecting`; once the control connection is back, a new one is made,
//   and its data follows its `ready`. States and data share the one port.

import { createHash } from "node:crypto";

import { encodeJson, FRAME_JSON, FrameDecoder } from "@cupper/host-client";
import { CLIENT_NOTIFICATIONS, METHODS, type InputEvidence } from "@cupper/protocol";
import type { Handshake, HostPort } from "@cupper/shell-api";

import type { ToWindow } from "../../shared/ipc";
import type { InputGuard } from "../input";

/** Unacknowledged bytes past which the bridge stops reading (design 5.7.1). */
export const MAX_UNACKED_BYTES = 8 * 1024 * 1024;
/** How long inbound bytes gather before they go to the window. */
export const BATCH_MS = 16;
/** The scopes a window may use; `privileged`, `test` and `none` stay local (design 5.7.8). */
const WINDOW_SCOPES = ["read", "write", "approve", "ui_local"];
/** The commands that need real input (design 5.7.8, rule 4). */
const REAL_INPUT = new Set(["interaction/respond", "git/revert"]);

/** A handshaken data connection. */
export interface DataConnection {
  port: HostPort;
  handshake: Handshake;
  /** What came after the response to `initialize`. */
  rest: Uint8Array;
  pause(): void;
  resume(): void;
  onClose(cb: () => void): void;
}

/** The main process's end of the window's port. */
export interface WindowPort {
  post(message: ToWindow): void;
  onMessage(cb: (message: unknown) => void): void;
  close(): void;
}

export interface BridgeDeps {
  /** A new data connection: the launcher's, with role `window`. */
  connect(): Promise<DataConnection>;
  /** Resolves once the control connection is ready: it reconnects first (design 5.7.1). */
  controlReady(): Promise<void>;
  /** `setTimeout` in the app. */
  schedule(task: () => void, ms: number): void;
  /** The window's real input. */
  input: Pick<InputGuard, "take">;
}

/** Whether a window may send `method`: as a request when it has an `id`. */
export function allowed(method: string, request: boolean): boolean {
  const catalog = request ? METHODS : CLIENT_NOTIFICATIONS;
  const scope = Object.hasOwn(catalog, method) ? catalog[method]?.scope : undefined;
  return scope !== undefined && WINDOW_SCOPES.includes(scope);
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** A command's parameters, `input` left out, in one form whatever the key order. */
function paramsHash(params: Record<string, unknown>): string {
  const rest = Object.fromEntries(Object.entries(params).filter(([key]) => key !== "input"));
  const sorted = (_key: string, value: unknown) =>
    isRecord(value)
      ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => (a < b ? -1 : 1)))
      : value;
  return createHash("sha256").update(JSON.stringify(rest, sorted)).digest("hex");
}

/** Whether `value` holds an integer the parse may have changed. */
function unsafeInteger(value: unknown): boolean {
  if (typeof value === "number") {
    return Number.isInteger(value) && !Number.isSafeInteger(value);
  }
  return typeof value === "object" && value !== null && Object.values(value).some(unsafeInteger);
}

const error = (code: string, reason?: string) => ({
  code: -32000,
  message: reason === undefined ? code : `${code}/${reason}`,
  data: reason === undefined ? { code } : { code, reason },
});

export class WindowBridge {
  readonly #window: WindowPort;
  readonly #deps: BridgeDeps;
  #conn: DataConnection | undefined;
  #closed = false;
  #batch: Uint8Array[] = [];
  #batchBytes = 0;
  #unacked = 0;
  #paused = false;
  /** Uploaded bytes by blob, until the `blob/put` that commits it. */
  readonly #uploads = new Map<string, number>();
  /** Forwarded commands that needed input, by `command_id`, until the host answers (rule 9). */
  readonly #released = new Map<string, { hash: string; input: InputEvidence }>();
  /** Their `command_id`s by request `id`, on this connection. */
  readonly #awaiting = new Map<unknown, string>();
  /** Follows the inbound frames to see those answers; the bytes go on as they came. */
  #frames = new FrameDecoder();

  constructor(window: WindowPort, deps: BridgeDeps) {
    this.#window = window;
    this.#deps = deps;
    window.onMessage((message) => this.#fromWindow(message));
  }

  /** Makes the first connection; its handshake goes back with the port. */
  async open(): Promise<Handshake> {
    const conn = await this.#deps.connect();
    if (this.#closed) {
      conn.port.close();
      throw new Error("the window went away");
    }
    this.#use(conn);
    return conn.handshake;
  }

  /** The document went away: the connection with it, for good. */
  close(): void {
    this.#closed = true;
    this.#batch = [];
    this.#conn?.port.close();
    this.#window.close();
  }

  /** Drops the connection as a host would; the bridge reconnects (e2e). */
  disconnect(): void {
    this.#conn?.port.close();
  }

  #use(conn: DataConnection): void {
    this.#conn = conn;
    this.#paused = false;
    this.#frames = new FrameDecoder();
    this.#awaiting.clear();
    conn.onClose(() => void this.#lost(conn));
    if (conn.rest.length > 0) {
      this.#inbound(conn.rest);
    }
    conn.port.onData((chunk) => this.#inbound(new Uint8Array(chunk)));
    this.#throttle();
  }

  async #lost(conn: DataConnection): Promise<void> {
    if (conn !== this.#conn || this.#closed) {
      return;
    }
    this.#conn = undefined;
    this.#flush();
    this.#window.post({ type: "state", state: { status: "reconnecting" } });
    let next;
    try {
      await this.#deps.controlReady();
      next = await this.#deps.connect();
    } catch (err) {
      const reason = (err as { reason?: unknown } | null)?.reason;
      const failed = typeof reason === "string" ? reason : "host_unreachable";
      this.#window.post({ type: "state", state: { status: "failed", reason: failed } });
      return;
    }
    if (this.#closed) {
      next.port.close();
      return;
    }
    this.#window.post({ type: "state", state: { status: "ready" }, handshake: next.handshake });
    this.#use(next);
  }

  #inbound(bytes: Uint8Array): void {
    if (this.#closed) {
      return;
    }
    this.#batch.push(bytes);
    this.#batchBytes += bytes.length;
    if (this.#batch.length === 1) {
      this.#deps.schedule(() => this.#flush(), BATCH_MS);
    }
    this.#throttle();
    this.#answered(bytes);
  }

  /** Drops the retransmit records of the commands the host answered (rule 9). */
  #answered(bytes: Uint8Array): void {
    try {
      this.#frames.push(bytes);
      for (let frame = this.#frames.next(); frame; frame = this.#frames.next()) {
        if (frame.type !== FRAME_JSON || this.#awaiting.size === 0) {
          continue;
        }
        const reply: unknown = JSON.parse(new TextDecoder().decode(frame.payload));
        const id = isRecord(reply) && !("method" in reply) ? reply.id : undefined;
        const command = this.#awaiting.get(id);
        if (command !== undefined) {
          this.#awaiting.delete(id);
          this.#released.delete(command);
        }
      }
    } catch {
      // Not frames the window can use either; it hears so from its own decoder.
      this.#frames = new FrameDecoder();
    }
  }

  #flush(): void {
    if (this.#batch.length === 0 || this.#closed) {
      return;
    }
    const chunk = new Uint8Array(this.#batchBytes);
    let at = 0;
    for (const bytes of this.#batch) {
      chunk.set(bytes, at);
      at += bytes.length;
    }
    this.#batch = [];
    this.#batchBytes = 0;
    this.#unacked += chunk.length;
    this.#window.post({ type: "data", chunk: chunk.buffer });
  }

  /** Stops reading past the limit, and reads again below it. */
  #throttle(): void {
    const over = this.#unacked + this.#batchBytes > MAX_UNACKED_BYTES;
    if (over !== this.#paused && this.#conn) {
      this.#paused = over;
      if (over) {
        this.#conn.pause();
      } else {
        this.#conn.resume();
      }
    }
  }

  #fromWindow(message: unknown): void {
    if (!isRecord(message) || this.#closed) {
      return;
    }
    const { type } = message;
    if (type === "send" && typeof message.json === "string") {
      this.#send(message.json);
    } else if (type === "ack" && typeof message.bytes === "number" && message.bytes >= 0) {
      this.#unacked = Math.max(0, this.#unacked - message.bytes);
      this.#throttle();
    } else if (type === "blob" && message.bytes instanceof ArrayBuffer) {
      const { hash, offset, last, bytes } = message;
      if (typeof hash === "string" && typeof offset === "number" && typeof last === "boolean") {
        this.#blob(hash, offset, last, bytes);
      }
    } else if (type === "close") {
      this.close();
    }
  }

  #send(json: string): void {
    let message: unknown;
    try {
      message = JSON.parse(json);
    } catch {
      return;
    }
    if (!isRecord(message) || typeof message.method !== "string") {
      return;
    }
    const { method } = message;
    const request = "id" in message;
    const refuse = (reason?: string) => {
      if (request) {
        const reply = { jsonrpc: "2.0", id: message.id, error: error("forbidden", reason) };
        this.#inbound(encodeJson(reply));
      }
    };
    if (method === "initialize") {
      return refuse("already_initialized");
    }
    if (!allowed(method, request) || unsafeInteger(message)) {
      return refuse();
    }
    // Rule 4: any command uses up the input not used yet.
    const input = METHODS[method]?.idempotent === "command" ? this.#deps.input.take() : undefined;
    if (REAL_INPUT.has(method)) {
      const params = isRecord(message.params) ? message.params : {};
      const command = typeof params.command_id === "string" ? params.command_id : undefined;
      const hash = paramsHash(params);
      const released = command === undefined ? undefined : this.#released.get(command);
      const evidence = input ?? (released?.hash === hash ? released.input : undefined);
      if (!request || command === undefined || evidence === undefined) {
        return refuse("user_activation_required");
      }
      message.params = { ...params, input: evidence };
      this.#released.set(command, { hash, input: evidence });
      this.#awaiting.set(message.id, command);
    }
    if (method === "blob/put" && isRecord(message.params)) {
      this.#uploads.delete(String(message.params.hash));
    }
    try {
      this.#conn?.port.send(JSON.stringify(message));
    } catch {
      // Larger than a frame: the host would not take it either.
      refuse("too_large");
    }
  }

  /** One `0x02` chunk, within the frame and this window's pending uploads. */
  #blob(hash: string, offset: number, last: boolean, bytes: ArrayBuffer): void {
    const pending = [...this.#uploads.values()].reduce((sum, n) => sum + n, 0);
    const limit = this.#conn?.handshake.limits.max_pending_upload_bytes ?? 0;
    if (!this.#conn || pending + bytes.byteLength > limit) {
      return;
    }
    try {
      // Encoding checks the header and the frame size.
      this.#conn.port.sendBlob(hash, offset, last, bytes);
    } catch {
      return;
    }
    this.#uploads.set(hash, (this.#uploads.get(hash) ?? 0) + bytes.byteLength);
  }
}
