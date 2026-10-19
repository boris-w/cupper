// The connection object of design 5.7.6: the state machine, frames in and
// out, requests matched to responses, and commands that survive a lost
// connection. A driver attaches it to a HostPort after each handshake: the
// renderer through `ShellAPI.host` (`connectShell`), the main process over its
// sockets (T075). Stream sync (`subscribe`) builds on `call`, `onNotification` and `onState`.

import { zRpcError, type RpcError } from "@cupper/protocol";
import type {
  ConnectionFailure,
  ConnectionState,
  Handshake,
  HostPort,
  Unsubscribe,
} from "@cupper/shell-api";

import {
  decodeBlobChunk,
  FRAME_BLOB,
  FRAME_JSON,
  FrameDecoder,
  FrameError,
  type BlobChunk,
} from "./frame";
import { Listeners } from "./listeners";
import { StreamSync, type StreamSink, type SyncCounters } from "./sync";

export type ClientState = ConnectionState | { status: "closed" };

/** How a call ended. `lost`: the connection went before a response came. */
export type Outcome = { result: unknown } | { error: RpcError } | { lost: true };

export class RpcFailure extends Error {
  override name = "RpcFailure";
  constructor(readonly error: RpcError) {
    super(error.message);
  }
}

export class ConnectionLost extends Error {
  override name = "ConnectionLost";
}

export interface HostClientOptions {
  /** A new `command_id`; a UUIDv7 by default. */
  commandId?: () => string;
  /** Protocol oddities: broken frames, messages that are not JSON-RPC, responses nobody waits for. */
  log?: (message: string) => void;
}

interface Call {
  method: string;
  params: unknown;
  done: (outcome: Outcome) => void;
  /** Sent again, with the same `command_id`, when the connection comes back. */
  command: boolean;
}

/** A UUIDv7 (RFC 9562): 48 bits of Unix milliseconds, then random bits. */
export function uuidv7(now = Date.now(), random = crypto.getRandomValues(new Uint8Array(10))) {
  const bytes = new Uint8Array(16);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(now) << 16n);
  bytes.set(random, 6);
  bytes[6] = 0x70 | ((bytes[6] ?? 0) & 0x0f);
  bytes[8] = 0x80 | ((bytes[8] ?? 0) & 0x3f);
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  return [8, 12, 16, 20].reduceRight((id, at) => `${id.slice(0, at)}-${id.slice(at)}`, hex);
}

export class HostClient {
  #state: ClientState = { status: "connecting" };
  #handshake: Handshake | undefined;
  #everReady = false;
  #port: HostPort | undefined;
  #offData: Unsubscribe | undefined;
  #decoder = new FrameDecoder();
  /** Chunks that arrived while an earlier one was being handled. */
  #inbox: Uint8Array[] = [];
  #nextId = 1;
  readonly #calls = new Map<number, Call>();
  /** Commands waiting for a connection, in the order they were made. */
  #queued: Call[] = [];
  readonly #states = new Listeners<[ClientState, Handshake | undefined]>();
  readonly #notifications = new Listeners<[string, unknown]>();
  readonly #blobs = new Listeners<[BlobChunk]>();
  readonly #commandId: () => string;
  readonly #log: (message: string) => void;
  readonly #streams = new Map<string, StreamSync>();
  readonly #counters: SyncCounters = { gaps: 0, duplicates: 0, resets: 0 };

  constructor(options: HostClientOptions = {}) {
    this.#commandId = options.commandId ?? (() => uuidv7());
    this.#log = options.log ?? ((message) => console.warn(`host-client: ${message}`));
    // Registered first: streams see each notification and state before other listeners do.
    this.onNotification((method, params) => this.#route(method, params));
    this.onState((state) => {
      for (const sync of this.#streams.values()) {
        if (state.status === "ready") {
          this.#subscribe(sync, sync.subscribe());
        } else {
          sync.detach();
        }
      }
    });
  }

  /** Gaps, duplicates and resets over every stream so far. */
  get counters(): SyncCounters {
    return { ...this.#counters };
  }

  /**
   * Follows `stream` into `sink` until the returned function is called; see
   * `StreamSync` for the rules. After a reconnect it resumes from its cursor.
   */
  subscribe(stream: string, sink: StreamSink): Unsubscribe {
    if (this.#streams.has(stream)) {
      throw new Error(`${stream} is subscribed already`);
    }
    const sync = new StreamSync(stream, sink, this.#counters, this.#log);
    this.#streams.set(stream, sync);
    if (this.#state.status === "ready") {
      this.#subscribe(sync, sync.subscribe());
    }
    return () => {
      if (this.#streams.get(stream) === sync) {
        this.#streams.delete(stream);
        this.call("stream/unsubscribe", { stream }, () => undefined);
      }
    };
  }

  get state(): ClientState {
    return this.#state;
  }

  /** The result of the latest handshake. */
  get handshake(): Handshake | undefined {
    return this.#handshake;
  }

  onState(cb: (state: ClientState, handshake?: Handshake) => void): Unsubscribe {
    return this.#states.add(cb);
  }

  /** Every notification from the host, `stream/*` included, in the order it came. */
  onNotification(cb: (method: string, params: unknown) => void): Unsubscribe {
    return this.#notifications.add(cb);
  }

  /** The `0x02` chunks that `blob/get` sends ahead of its response. */
  onBlobChunk(cb: (chunk: BlobChunk) => void): Unsubscribe {
    return this.#blobs.add(cb);
  }

  /** The driver is dialing or handshaking. After the first `ready` this is all `reconnecting`. */
  progress(step: "connecting" | "handshaking"): void {
    this.#set({ status: this.#everReady ? "reconnecting" : step });
  }

  /** A handshake succeeded on `port`; `rest` is what came after its response. */
  attach(port: HostPort, handshake: Handshake, rest?: Uint8Array): void {
    if (this.#state.status === "closed") {
      port.close();
      return;
    }
    this.#lose();
    this.#port = port;
    this.#handshake = handshake;
    this.#everReady = true;
    this.#offData = port.onData((chunk) => this.#receive(port, new Uint8Array(chunk)));
    this.#set({ status: "ready" }, handshake);
    if (rest?.length) {
      this.#receive(port, rest);
    }
    const queued = this.#queued;
    this.#queued = [];
    queued.forEach((call) => this.#start(call));
  }

  /** The connection went away: requests in flight fail, commands wait for the next `attach`. */
  detach(): void {
    if (this.#state.status !== "closed") {
      this.#lose();
      this.#set({ status: this.#everReady ? "reconnecting" : "connecting" });
    }
  }

  fail(reason: ConnectionFailure): void {
    if (this.#state.status !== "closed") {
      this.#lose();
      this.#set({ status: "failed", reason });
    }
  }

  /** Closes the port for good; whatever still waits, commands included, ends as lost. */
  close(): void {
    const port = this.#port;
    this.#lose();
    port?.close();
    this.#set({ status: "closed" });
    const queued = this.#queued;
    this.#queued = [];
    queued.forEach((call) => call.done({ lost: true }));
  }

  /**
   * Sends a request; `done` gets its outcome synchronously, in its place
   * among the notifications. Without a connection it is lost at once.
   */
  call(method: string, params: unknown, done: (outcome: Outcome) => void): void {
    this.#start({ method, params, done, command: false });
  }

  request<R = unknown>(method: string, params: unknown = {}): Promise<R> {
    return new Promise((resolve, reject) => this.call(method, params, settle(resolve, reject)));
  }

  /**
   * A method with side effects (design 5.3.1). It gets a new `command_id`
   * unless `params` has one, and is sent again with it after a reconnect
   * until a response comes (NFR-2).
   */
  command<R = unknown>(method: string, params: Record<string, unknown>): Promise<R> {
    const command_id = params.command_id ?? this.#commandId();
    return new Promise((resolve, reject) =>
      this.#start({
        method,
        params: { ...params, command_id },
        done: settle(resolve, reject),
        command: true,
      }),
    );
  }

  #subscribe(sync: StreamSync, params: object): void {
    this.call("stream/subscribe", params, (outcome) => {
      // A lost call waits for the next `ready`; a stream unsubscribed meanwhile is gone.
      if ("lost" in outcome || this.#streams.get(sync.stream) !== sync) {
        return;
      }
      if ("error" in outcome) {
        sync.failed(outcome.error);
        return;
      }
      const again = sync.subscribed(outcome.result);
      if (again) {
        this.#subscribe(sync, again);
      }
    });
  }

  #route(method: string, params: unknown): void {
    const stream = (params as { stream?: unknown } | null)?.stream;
    const sync = typeof stream === "string" ? this.#streams.get(stream) : undefined;
    const again =
      sync &&
      (method === "stream/events"
        ? sync.events(params)
        : method === "stream/snapshot"
          ? sync.snapshot(params)
          : method === "stream/reset"
            ? sync.reset()
            : undefined);
    if (sync && again) {
      this.#subscribe(sync, again);
    }
  }

  #set(state: ClientState, handshake?: Handshake): void {
    if (this.#state.status === "closed") {
      return;
    }
    const same = JSON.stringify(state) === JSON.stringify(this.#state);
    this.#state = state;
    if (!same || state.status === "ready") {
      this.#states.emit(state, handshake);
    }
  }

  #start(call: Call): void {
    const port = this.#port;
    if (port && this.#state.status === "ready") {
      const id = this.#nextId++;
      this.#calls.set(id, call);
      port.send(JSON.stringify({ jsonrpc: "2.0", id, method: call.method, params: call.params }));
    } else if (call.command && this.#state.status !== "closed") {
      this.#queued.push(call);
    } else {
      call.done({ lost: true });
    }
  }

  /** Lets go of the port; calls in flight are lost, except commands, which queue up again first. */
  #lose(): void {
    this.#offData?.();
    this.#offData = undefined;
    this.#port = undefined;
    this.#inbox = [];
    const calls = [...this.#calls.values()];
    this.#calls.clear();
    this.#queued = [...calls.filter((c) => c.command), ...this.#queued];
    calls.filter((c) => !c.command).forEach((c) => c.done({ lost: true }));
  }

  /** Handles chunks one at a time, even when a listener sends and the reply comes at once. */
  #receive(port: HostPort, bytes: Uint8Array): void {
    if (port !== this.#port) {
      return;
    }
    this.#inbox.push(bytes);
    if (this.#inbox.length > 1) {
      return;
    }
    while (this.#inbox.length > 0 && this.#port === port) {
      const chunk = this.#inbox[0] ?? new Uint8Array();
      this.#decoder.push(chunk);
      try {
        for (let frame = this.#decoder.next(); frame; frame = this.#decoder.next()) {
          this.#frame(frame.type, frame.payload);
        }
      } catch (err) {
        this.#inbox = [];
        if (!(err instanceof FrameError)) {
          throw err;
        }
        this.#log(`closing the connection: ${err.message}`);
        port.close();
        this.fail("protocol_error");
        return;
      }
      port.ack(chunk.length);
      this.#inbox.shift();
    }
  }

  #frame(type: number, payload: Uint8Array): void {
    if (type === FRAME_BLOB) {
      this.#blobs.emit(decodeBlobChunk(payload));
      return;
    }
    let message: unknown;
    try {
      message = type === FRAME_JSON ? JSON.parse(new TextDecoder().decode(payload)) : undefined;
    } catch {
      // Logged below.
    }
    if (typeof message !== "object" || message === null || Array.isArray(message)) {
      this.#log(`skipped a frame of type ${type} that is no JSON-RPC message`);
      return;
    }
    const { id, method, params, error } = message as Record<string, unknown>;
    if (typeof method === "string" && !("id" in message)) {
      this.#notifications.emit(method, params);
      return;
    }
    const call = typeof id === "number" ? this.#calls.get(id) : undefined;
    if (call === undefined || typeof method === "string") {
      this.#log(`skipped a message that answers nothing sent: ${String(id)}`);
      return;
    }
    this.#calls.delete(id as number);
    if (!("error" in message)) {
      call.done({ result: (message as { result?: unknown }).result });
      return;
    }
    const parsed = zRpcError.safeParse(error);
    call.done({
      error: parsed.success ? parsed.data : { code: -32603, message: "malformed error" },
    });
  }
}

function settle<R>(resolve: (result: R) => void, reject: (error: Error) => void) {
  return (outcome: Outcome) => {
    if ("result" in outcome) {
      resolve(outcome.result as R);
    } else {
      reject(
        "error" in outcome ? new RpcFailure(outcome.error) : new ConnectionLost("connection lost"),
      );
    }
  };
}
