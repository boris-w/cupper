import type { Unsubscribe } from "@cupper/shell-api";

/**
 * Versions by key, so that a component re-renders only when its entry
 * changes (design 5.7.6, `useSyncExternalStore`). Changes apply at once;
 * the listeners hear of them on `flush`, which the store calls once per
 * animation frame. The key `view` changes with every change.
 */
export class Versions {
  #counter = 0;
  readonly #versions = new Map<string, number>();
  readonly #listeners = new Map<string, Set<() => void>>();
  readonly #dirty = new Set<string>();

  of(key: string): number {
    return this.#versions.get(key) ?? 0;
  }

  subscribe(key: string, cb: () => void): Unsubscribe {
    const listeners = this.#listeners.get(key) ?? new Set();
    this.#listeners.set(key, listeners);
    const entry = () => cb();
    listeners.add(entry);
    return () => {
      listeners.delete(entry);
    };
  }

  touch(...keys: string[]): void {
    for (const key of [...keys, "view"]) {
      this.#versions.set(key, ++this.#counter);
      this.#dirty.add(key);
    }
  }

  /** Every key changes: the view was dropped. */
  touchAll(): void {
    this.touch(...this.#versions.keys(), ...this.#listeners.keys());
  }

  /** The keys changed since the last flush, each listener called once. */
  flush(): ReadonlySet<string> {
    const dirty = new Set(this.#dirty);
    this.#dirty.clear();
    for (const key of dirty) {
      this.#listeners.get(key)?.forEach((cb) => cb());
    }
    return dirty;
  }
}
