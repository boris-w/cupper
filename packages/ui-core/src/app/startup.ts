// The renderer's marks of a start (design 5.9.6, T262), for measuring the
// cold start to usable: the window's data connection handshaken, the index
// snapshot applied, and `cupper:ready`, the index applied and the composer
// usable. The composer's text area is never disabled, so usable is mounted.
// Each mark once per document; marking reads the start, it changes nothing.

export const MARKS = {
  connected: "cupper:connected",
  index: "cupper:index",
  ready: "cupper:ready",
} as const;

export class Startup {
  readonly #marked = new Set<string>();
  #index = false;
  #composer = false;

  constructor(private readonly mark: (name: string) => void = (name) => performance.mark(name)) {}

  /** The window's host client is ready: its data connection handshaken. */
  connected(): void {
    this.#once(MARKS.connected);
  }

  /** The index stream went live: its snapshot is in the view. */
  indexApplied(): void {
    this.#index = true;
    this.#once(MARKS.index);
    this.#ready();
  }

  composerMounted(): void {
    this.#composer = true;
    this.#ready();
  }

  #ready(): void {
    if (this.#index && this.#composer) this.#once(MARKS.ready);
  }

  #once(name: string): void {
    if (this.#marked.has(name)) return;
    this.#marked.add(name);
    this.mark(name);
  }
}

/** The document's start. */
export const startup = new Startup();
