// A stream's store (design 5.7.6): its view, fed by the host-client as each
// message arrives, so the client acknowledges what it applied. Only telling
// the components is put off: one `versions.flush()` per animation frame.

import type { StreamSink, StreamView } from "@cupper/host-client";
import type { Event, EventBody, RpcError } from "@cupper/protocol";
import { useCallback, useSyncExternalStore } from "react";

type View = StreamView<unknown>;

/** Flushes the views that changed, once in the next animation frame. */
export class Frames {
  readonly #dirty = new Set<View>();
  #pending = false;

  // Called as a plain function: the browser's throws `Illegal invocation` when
  // its `this` is not the window.
  constructor(
    private readonly frame: (cb: () => void) => void = (cb) => requestAnimationFrame(cb),
  ) {}

  mark(view: View): void {
    this.#dirty.add(view);
    if (!this.#pending) {
      this.#pending = true;
      this.frame(() => this.#flush());
    }
  }

  #flush(): void {
    this.#pending = false;
    const dirty = [...this.#dirty];
    this.#dirty.clear();
    dirty.forEach((view) => view.versions.flush());
  }
}

/** The sink of one stream: its view, a frame after each change, and each live event to `onEvent`. */
export class StreamStore<V extends View> implements StreamSink {
  constructor(
    readonly stream: string,
    readonly view: V,
    private readonly frames: Frames,
    private readonly onEvent?: (stream: string, body: EventBody) => void,
  ) {}

  reset(): void {
    this.view.reset();
    this.frames.mark(this.view);
  }

  apply(body: EventBody, event?: Event): void {
    this.view.apply(body, event);
    this.frames.mark(this.view);
    if (event) {
      this.onEvent?.(this.stream, body);
    }
  }

  live(): void {
    this.view.live();
    this.frames.mark(this.view);
  }

  failed(error: RpcError): void {
    this.view.failed(error);
    this.frames.mark(this.view);
  }
}

/**
 * Re-renders when entry `key` of `view` changes (`item:<id>`, `session:<id>`,
 * `tail`, ...; `view` for any change, `complete` and `error` included). The
 * component reads what it shows from the view itself.
 */
export function useEntry(view: View, key: string): number {
  const subscribe = useCallback((cb: () => void) => view.versions.subscribe(key, cb), [view, key]);
  return useSyncExternalStore(subscribe, () => view.versions.of(key));
}
