// The two scroll rules of the message stream (design 5.7.6): while the user
// is at the bottom the list follows what streams in; history put in front
// keeps the row at the top of the viewport where it was, as the rows in front
// are measured too, until the user scrolls (T263). Only the user leaves
// the bottom (T245): a change of position within `INTENT_MS` of their wheel,
// touch or key, or while they hold the scrollbar; the browser moves the list
// too, as rows lay out again, and that only ever brings it back to the bottom.
// The app's own jumps leave it with `leave()`.

/** How close to the bottom still counts as at the bottom. */
export const STICK_PX = 24;
/** How long after the user's wheel, touch or key a change of position is theirs. */
export const INTENT_MS = 500;

export class ScrollKeeper {
  /** At the bottom: follow the growth. */
  stuck = true;
  /** When the user last did something that scrolls; `Infinity` while they hold the scrollbar. */
  #intent = -Infinity;
  /** The held row: `delta` below the viewport's top; `start`, where it was once rows went in front. */
  #anchor: { key: string; index: number; delta: number; start?: number } | undefined;
  /** Where the list was when last scrolled or put. */
  #top: number | undefined;

  /** The user did something that scrolls at `now`; `holding`: the scrollbar, until `letGo()`. */
  intend(now: number, holding = false): void {
    this.#intent = holding ? Infinity : now;
    this.#anchor = undefined;
  }

  /** The user let go of the scrollbar at `now`. */
  letGo(now: number): void {
    if (this.#intent === Infinity) this.#intent = now;
  }

  /** The user went back to the end, by the ↓ button or by sending: follow it again (T256). */
  follow(): void {
    this.stuck = true;
    this.#intent = -Infinity;
    this.#anchor = undefined;
  }

  /** The app jumped away from the bottom (a row, the plan): stop following. */
  leave(): void {
    this.stuck = false;
    this.#anchor = undefined;
  }

  /** The list scrolled at `now`; whether it follows. Only the user's scroll leaves the bottom. */
  scrolled(top: number, height: number, client: number, now: number): boolean {
    // Moved by someone else, the browser's focus or find: the held row goes.
    if (this.#top !== undefined && Math.abs(top - this.#top) > 1) this.#anchor = undefined;
    this.#top = top;
    if (height - top - client <= STICK_PX) {
      this.stuck = true;
    } else if (now - this.#intent <= INTENT_MS) {
      this.stuck = false;
    }
    return this.stuck;
  }

  /** The list was put at `top`, by the list or its virtualizer: not a scroll. */
  put(top: number): void {
    this.#top = top;
  }

  /**
   * Whether to put the list at the bottom now. A list that is not where it
   * was last scrolled or put was scrolled, and its event has not come yet:
   * the virtualizer may have read it on a timer and rendered first (T208).
   */
  follows(top: number, height: number, client: number, now: number): boolean {
    if (this.#top !== undefined && Math.abs(top - this.#top) > 1) {
      this.scrolled(top, height, client, now);
    }
    return this.stuck;
  }

  /**
   * Before rows go in front: row `key`, at `index`, starts at `start`; the
   * viewport at `top`. Held until the user scrolls, or the app jumps. Only
   * ever held at the top of a list the user scrolled up, for a page of
   * history: following the bottom, or what streams in below the held row,
   * does not move it.
   */
  hold(key: string, index: number, start: number, top: number): void {
    this.#anchor = { key, index, delta: start - top };
  }

  /**
   * Once rows went in front of the held row, and again each time it moves
   * after, as the rows in front are measured: the scroll top that puts it
   * back where it was. Otherwise `undefined`.
   *
   * `locate` reads the measurements as of the render. Rows measured after it
   * move the held row only in a later render, and that render puts it back
   * in the same commit, so the move is never painted. The virtualizer keeps
   * a row in place itself for the rows above the viewport it measures, but
   * not for those it measures before it has seen the scroll that put the
   * held row back (T263). The two never add up: this is where the held row
   * should be, not a move by a difference, and once the virtualizer has kept
   * it in place it is already there.
   */
  restore(
    locate: (key: string) => { index: number; start: number } | undefined,
  ): number | undefined {
    const anchor = this.#anchor;
    const now = anchor && locate(anchor.key);
    if (
      anchor === undefined ||
      now === undefined ||
      (anchor.start === undefined ? now.index === anchor.index : now.start === anchor.start)
    ) {
      return undefined;
    }
    anchor.start = now.start;
    return now.start - anchor.delta;
  }
}
