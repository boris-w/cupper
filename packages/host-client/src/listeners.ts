import type { Unsubscribe } from "@cupper/shell-api";

/** Subscribers of one event. A callback removed during an emit is not called. */
export class Listeners<Args extends unknown[]> {
  readonly #entries = new Set<{ cb: (...args: Args) => void }>();

  add(cb: (...args: Args) => void): Unsubscribe {
    const entry = { cb };
    this.#entries.add(entry);
    return () => {
      this.#entries.delete(entry);
    };
  }

  emit(...args: Args): void {
    for (const entry of [...this.#entries]) {
      if (this.#entries.has(entry)) {
        entry.cb(...args);
      }
    }
  }
}
