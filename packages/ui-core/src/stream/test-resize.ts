// For the stream's tests only: jsdom has no ResizeObserver. Each element
// observed, with a resize to call by hand as a real one would come.

export const observed: { target: Element; resize: () => void }[] = [];

export function installResizeObserver(): void {
  globalThis.ResizeObserver = class {
    #callback: ResizeObserverCallback;
    constructor(callback: ResizeObserverCallback) {
      this.#callback = callback;
    }
    observe(target: Element) {
      const entry = { target } as ResizeObserverEntry;
      observed.push({ target, resize: () => this.#callback([entry], this as never) });
    }
    unobserve = () => undefined;
    disconnect = () => undefined;
  } as unknown as typeof ResizeObserver;
}
