// Stream sync (design 5.3.3, 5.7.6; T014's rulings in 5.3.11): one stream
// as the client has it. The same rules as the Rust `Mirror`
// (`crates/cupper-cli/src/mirror.rs`), but the view lives in the caller's
// `StreamSink`:
//
// - Only a `replay` that fits the cursor keeps the view and the cursor. Any
//   other subscription result, a snapshot whatever the reason or a mode this
//   version does not know, drops both: after `epoch_changed` seqs may be reused.
// - Snapshot pages must match the subscription's `at_seq` and `epoch` and come in order.
// - Events must be contiguous; after a gap or a duplicate the stream subscribes
//   again with its cursor. After a ViewError (the sink throws), or an event
//   without a readable envelope, it starts over without one.
// - `stream/reset` keeps the cursor.
// - Under `non_delta` validation (production builds, design 5.7.6), an
//   `item.delta` skips zod: only its `seq` and `from` are checked.
// - Until the result of a subscription arrives, whatever else comes for the
//   stream belongs to an older subscription and is ignored.
//
// Each method returns the parameters to subscribe again with, if it must.

import {
  isKnownEvent,
  isKnownEventType,
  zEvent,
  zEventBody,
  zStreamEventsParams,
  zStreamSnapshotParams,
  zStreamSubscribeResult,
  type Event,
  type EventBody,
  type RpcError,
  type StreamSubscribeParams,
} from "@cupper/protocol";
import * as z from "zod";

/** Where the view of a stream is fed; T072's views and T080's stores implement it. */
export interface StreamSink {
  /** Drop the view: snapshot ops follow, then `live`. */
  reset(): void;
  /** A snapshot op, or a live event's body with its event. Throws when the view cannot apply it. */
  apply(body: EventBody, event?: Event): void;
  /** The view is complete and events follow: after the last snapshot page, or a replay from the cursor. */
  live(): void;
  /** `stream/subscribe` failed, e.g. `not_found` for a removed session. The stream waits. */
  failed?(error: RpcError): void;
}

/** Counted over every stream of a client, for tests to assert (design 5.7.6, 5.9.4). */
export interface SyncCounters {
  /** Events whose first seq skipped past the cursor. */
  gaps: number;
  /** Events whose first seq was not after the cursor. */
  duplicates: number;
  /** `stream/reset` from the host, and every complete view the client dropped. */
  resets: number;
}

/** Which events zod checks: all of them, or all but `item.delta`. */
export type Validation = "all" | "non_delta";

/** An `item.delta` that `validation` lets through without zod. */
export const trusted = (validation: Validation, type: unknown) =>
  validation === "non_delta" && type === "item.delta";

const integer = (value: unknown) => Number.isSafeInteger(value);

export interface Cursor {
  epoch: number;
  seq: number;
}

type State =
  | { kind: "waiting" }
  | { kind: "loading"; atSeq: number; epoch: number; page: number }
  | { kind: "live" };

// The events and ops one by one: a malformed one must not fail the others (handoff from T015).
const zEvents = zStreamEventsParams.extend({ events: z.array(z.unknown()) });
const zPage = zStreamSnapshotParams.extend({ ops: z.array(z.unknown()) });

export class StreamSync {
  #state: State = { kind: "waiting" };
  /** Where the view stands; none until a snapshot completes. */
  #cursor: Cursor | undefined;

  constructor(
    readonly stream: string,
    private readonly sink: StreamSink,
    private readonly counters: SyncCounters,
    private readonly log: (message: string) => void,
    private readonly validation: Validation = "all",
  ) {}

  get cursor(): Cursor | undefined {
    return this.#cursor && { ...this.#cursor };
  }

  get live(): boolean {
    return this.#state.kind === "live";
  }

  /** Parameters of `stream/subscribe`, with the cursor when there is one. Until its result, data is ignored. */
  subscribe(): StreamSubscribeParams {
    this.#state = { kind: "waiting" };
    const cursor = this.#cursor;
    return cursor
      ? { stream: this.stream, after_seq: cursor.seq, epoch: cursor.epoch }
      : { stream: this.stream };
  }

  /** The connection went away; the cursor stays for the next subscription. */
  detach(): void {
    this.#state = { kind: "waiting" };
  }

  subscribed(raw: unknown): StreamSubscribeParams | undefined {
    const result = zStreamSubscribeResult.safeParse(raw);
    if (!result.success) {
      this.failed({ code: -32603, message: "malformed stream/subscribe result" });
      return undefined;
    }
    const { mode, at_seq, epoch } = result.data;
    if (mode === "replay") {
      if (this.#cursor?.epoch !== epoch || this.#cursor.seq !== at_seq) {
        return this.#restart();
      }
      this.#state = { kind: "live" };
      this.sink.live();
      return undefined;
    }
    this.#drop();
    this.#state = { kind: "loading", atSeq: at_seq, epoch, page: 0 };
    return undefined;
  }

  failed(error: RpcError): void {
    this.sink.failed?.(error);
  }

  snapshot(raw: unknown): StreamSubscribeParams | undefined {
    const state = this.#state;
    // Waiting: an older subscription's page. Live: out of place, and ignored as in Rust.
    if (state.kind !== "loading") {
      return undefined;
    }
    const page = zPage.safeParse(raw);
    if (
      !page.success ||
      page.data.at_seq !== state.atSeq ||
      page.data.epoch !== state.epoch ||
      page.data.page !== state.page
    ) {
      return this.#restart();
    }
    for (const op of page.data.ops) {
      const body = zEventBody.safeParse(op);
      // Like an unknown type, an op without even a type changes nothing.
      if (!body.success) {
        this.log(`${this.stream}: skipped a snapshot op without a type`);
      } else if (!this.#apply(body.data)) {
        return this.#restart();
      }
    }
    state.page += 1;
    if (page.data.last) {
      this.#cursor = { epoch: state.epoch, seq: state.atSeq };
      this.#state = { kind: "live" };
      this.sink.live();
    }
    return undefined;
  }

  events(raw: unknown): StreamSubscribeParams | undefined {
    if (this.#state.kind === "waiting") {
      return undefined;
    }
    const parsed = zEvents.safeParse(raw);
    const cursor = this.#cursor;
    // Events only follow the last page.
    if (this.#state.kind === "loading" || !parsed.success || !cursor) {
      return this.#restart();
    }
    for (const item of parsed.data.events) {
      const event = this.#envelope(item);
      if (!event) {
        this.log(`${this.stream}: an event without a readable envelope`);
        return this.#restart();
      }
      const { seq } = event;
      const first = event.from ?? seq;
      if (first !== cursor.seq + 1 || seq < first) {
        this.counters[first <= cursor.seq ? "duplicates" : "gaps"] += 1;
        return this.subscribe();
      }
      cursor.seq = seq;
      if (!this.#apply(event, event)) {
        return this.#restart();
      }
    }
    return undefined;
  }

  /** `stream/reset`: the host dropped the subscription; subscribe again from the cursor. */
  reset(): StreamSubscribeParams | undefined {
    if (this.#state.kind === "waiting") {
      return undefined;
    }
    this.counters.resets += 1;
    return this.subscribe();
  }

  #envelope(item: unknown): Event | undefined {
    const raw = item as Partial<Record<string, unknown>> | null;
    if (trusted(this.validation, raw?.type)) {
      const fits = integer(raw?.seq) && (raw?.from === undefined || integer(raw.from));
      return fits ? (raw as Event) : undefined;
    }
    const event = zEvent.safeParse(item);
    return event.success ? event.data : undefined;
  }

  #apply(body: EventBody, event?: Event): boolean {
    if (
      !trusted(this.validation, body.type) &&
      isKnownEventType(body.type) &&
      !isKnownEvent(body)
    ) {
      this.log(`${this.stream}: malformed data in ${body.type}, shown as unknown`);
    }
    try {
      this.sink.apply(body, event);
      return true;
    } catch (err) {
      this.log(`${this.stream}: the view could not apply a ${body.type}: ${String(err)}`);
      return false;
    }
  }

  #drop(): void {
    if (this.#cursor) {
      this.counters.resets += 1;
    }
    this.#cursor = undefined;
    this.sink.reset();
  }

  /** Drops the view and the cursor, and subscribes for a snapshot. */
  #restart(): StreamSubscribeParams {
    this.#drop();
    return this.subscribe();
  }
}
