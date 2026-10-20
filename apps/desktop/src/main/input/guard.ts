// inputGuard (design 5.7.2; 5.7.8, "real input"): what the user did in one
// window's main frame, kept as the evidence a command may carry. The rules
// are pure: the clock comes in, so tests run them on fake time.
//
// - Keys come from `before-input-event` (keyDown), clicks from `input-event`
//   (mouseDown, then mouseUp of the same button within 1 s); either only while
//   the main frame has the focus (rule 1).
// - Enter, Space, Escape, ⌘Enter and the digits 1–9 count, unless composing,
//   repeated, or with ⌘, ⌃ or ⌥ other than ⌘Enter (rules 2, 6).
// - The arrows navigate: they do not count and are not typing, but they drop
//   the input not used yet (rule 2).
// - Any other key is typing: it drops the input not used yet, and for 1 s
//   Enter, Space and the digits after it do not count; ⌘Enter does (rule 3).
//   So is a key while composing.
// - For 1 s after the window gains focus no key counts; focus and blur drop
//   the input (rule 7).
// - A command uses up the input not used yet, once (rule 4): `take()`.

import type { InputEvidence, InputKey } from "@cupper/protocol";
import type { Input } from "electron";

/** How old evidence may be; how long typing and gaining focus hold keys back; the longest click. */
export const INPUT_MS = 1000;

export type KeyInput = Pick<
  Input,
  "type" | "key" | "isComposing" | "isAutoRepeat" | "meta" | "control" | "alt" | "shift"
>;

type KeyClass = "ignored" | "navigation" | "typing" | InputEvidence;

const MODIFIERS = new Set(["Shift", "Meta", "Control", "Alt", "CapsLock", "Fn"]);
const ARROWS = new Set(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]);
const KEYS = new Map<string, InputKey>([
  ["Enter", "enter"],
  [" ", "space"],
  ["Escape", "escape"],
]);

/** What a key press is to the rules. */
export function classify(input: KeyInput): KeyClass {
  if (input.isComposing) {
    return "typing";
  }
  if (MODIFIERS.has(input.key)) {
    return "ignored";
  }
  if (ARROWS.has(input.key)) {
    return "navigation";
  }
  const digit = /^[1-9]$/.test(input.key) ? Number(input.key) : undefined;
  const key = digit === undefined ? KEYS.get(input.key) : "digit";
  if (key === undefined) {
    return "typing";
  }
  const cmdEnter = key === "enter" && input.meta && !input.control && !input.alt;
  if (input.isAutoRepeat || ((input.meta || input.control || input.alt) && !cmdEnter)) {
    return "ignored";
  }
  if (cmdEnter) {
    return { kind: "key", key: "cmd_enter" };
  }
  return digit === undefined ? { kind: "key", key } : { kind: "key", key, digit };
}

export class InputGuard {
  readonly #now: () => number;
  #last: { evidence: InputEvidence; at: number } | undefined;
  #typedAt = -Infinity;
  #focusedAt = -Infinity;
  #down: { button: string; at: number } | undefined;

  /** `now`: milliseconds since the Unix epoch, as `ShellAPI.window.onFocusChange` tells them. */
  constructor(now: () => number) {
    this.#now = now;
  }

  /** The window gained focus; when. */
  focus(): number {
    this.#focusedAt = this.#now();
    this.#drop();
    return this.#focusedAt;
  }

  /** The window lost focus; when. */
  blur(): number {
    this.#drop();
    return this.#now();
  }

  /** A `before-input-event`; `mainFrame`: the main frame had the focus. */
  key(input: KeyInput, mainFrame: boolean): void {
    if (input.type !== "keyDown" || !mainFrame) {
      return;
    }
    const now = this.#now();
    const kind = classify(input);
    if (kind === "typing") {
      this.#typedAt = now;
      this.#last = undefined;
    } else if (kind === "navigation") {
      this.#last = undefined;
    } else if (kind !== "ignored") {
      const typing = now - this.#typedAt < INPUT_MS && kind.key !== "cmd_enter";
      if (!typing && now - this.#focusedAt >= INPUT_MS) {
        this.#last = { evidence: kind, at: now };
      }
    }
  }

  /** A mouse event of `input-event`; `mainFrame` as for keys, at mouseUp. */
  mouse(type: string, button: string | undefined, mainFrame: boolean): void {
    const now = this.#now();
    if (type === "mouseDown") {
      this.#down = button === undefined ? undefined : { button, at: now };
    } else if (type === "mouseUp") {
      const down = this.#down;
      this.#down = undefined;
      if (mainFrame && down !== undefined && down.button === button && now - down.at <= INPUT_MS) {
        this.#last = { evidence: { kind: "click" }, at: now };
      }
    }
  }

  /** Uses up the input not used yet; it is evidence when at most 1 s old (rule 4). */
  take(): InputEvidence | undefined {
    const last = this.#last;
    this.#last = undefined;
    return last !== undefined && this.#now() - last.at <= INPUT_MS ? last.evidence : undefined;
  }

  #drop(): void {
    this.#last = undefined;
    this.#down = undefined;
  }
}
