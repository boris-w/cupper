// The two scroll rules of the message stream (design 5.7.6): while the user
// is at the bottom the list follows what streams in; history put in front
// keeps the row at the top of the viewport where it was.

/** How close to the bottom still counts as at the bottom. */
export const STICK_PX = 24;

export class ScrollKeeper {
  /** At the bottom: follow the growth. */
  stuck = true;
  #anchor: { key: string; index: number; delta: number } | undefined;

  /** The user scrolled; whether the list is at the bottom now. */
  scrolled(top: number, height: number, client: number): boolean {
    this.stuck = height - top - client <= STICK_PX;
    return this.stuck;
  }

  /** Before rows go in front: row `key`, at `index`, starts at `start`; the viewport at `top`. */
  hold(key: string, index: number, start: number, top: number): void {
    this.#anchor = { key, index, delta: start - top };
  }

  /**
   * Once rows went in front of the held row: the scroll top that puts it
   * back where it was. Until then, `undefined`, and the row stays held.
   */
  restore(
    locate: (key: string) => { index: number; start: number } | undefined,
  ): number | undefined {
    const anchor = this.#anchor;
    const now = anchor && locate(anchor.key);
    if (anchor === undefined || now === undefined || now.index === anchor.index) {
      return undefined;
    }
    this.#anchor = undefined;
    return now.start - anchor.delta;
  }
}
