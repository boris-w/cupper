// The subscription policy of design 5.7.6: the index stream for as long as
// the window lives, the session on screen, and the 3 sessions seen last. An
// older session is unsubscribed and its view dropped; its draft and scroll
// position are UI state and stay. Opening a session that is not held, as
// from a notification, is a cache miss: it subscribes and shows loading.

import { IndexView, SessionView, type HostClient } from "@cupper/host-client";
import type { EventBody } from "@cupper/protocol";
import type { Unsubscribe } from "@cupper/shell-api";

import { StreamStore, type Frames } from "./stream";

/** Sessions kept besides the one on screen. */
export const RECENT = 3;

interface Held {
  store: StreamStore<SessionView>;
  off: Unsubscribe;
}

export class Subscriptions {
  readonly index: StreamStore<IndexView>;
  /** Least recently shown first; the one on screen, if any, last. */
  readonly #sessions = new Map<string, Held>();
  #current: string | undefined;
  #offIndex: Unsubscribe | undefined;

  constructor(
    private readonly client: HostClient,
    private readonly frames: Frames,
    private readonly onEvent?: (stream: string, body: EventBody) => void,
  ) {
    this.index = this.#store("index", new IndexView(client.validation));
  }

  /** Subscribes the index; after `close`, the session on screen too. */
  start(): void {
    this.#offIndex ??= this.client.subscribe(this.index.stream, this.index);
    this.show(this.#current);
  }

  /** The session on screen, or none; returns its store. */
  show(sessionId: string | undefined): StreamStore<SessionView> | undefined {
    this.#current = sessionId;
    let held = sessionId === undefined ? undefined : this.#sessions.get(sessionId);
    if (sessionId !== undefined) {
      this.#sessions.delete(sessionId);
      held ??= this.#subscribe(`session/${sessionId}`);
      this.#sessions.set(sessionId, held);
    }
    const keep = RECENT + (sessionId === undefined ? 0 : 1);
    for (const [id, old] of this.#sessions) {
      if (this.#sessions.size <= keep) {
        break;
      }
      this.#sessions.delete(id);
      old.off();
    }
    return held?.store;
  }

  /** The store of a session that is held. */
  session(sessionId: string): StreamStore<SessionView> | undefined {
    return this.#sessions.get(sessionId)?.store;
  }

  /** The sessions held, least recently shown first. */
  held(): string[] {
    return [...this.#sessions.keys()];
  }

  close(): void {
    this.#offIndex?.();
    this.#offIndex = undefined;
    this.#sessions.forEach(({ off }) => off());
    this.#sessions.clear();
  }

  #subscribe(stream: string): Held {
    const store = this.#store(stream, new SessionView(this.client.validation));
    return { store, off: this.client.subscribe(stream, store) };
  }

  #store<V extends IndexView | SessionView>(stream: string, view: V): StreamStore<V> {
    return new StreamStore(stream, view, this.frames, this.onEvent);
  }
}
