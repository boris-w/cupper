import { afterEach, describe, expect, test, vi } from "vitest";

import { InputGuard, INPUT_MS, type KeyInput } from "./guard";

afterEach(() => void vi.useRealTimers());

const keyDown = (key: string, more: Partial<KeyInput> = {}): KeyInput => ({
  type: "keyDown",
  key,
  isComposing: false,
  isAutoRepeat: false,
  meta: false,
  control: false,
  alt: false,
  shift: false,
  ...more,
});

/** A guard on Vitest's fake clock, its window focused long enough ago. */
function guarded() {
  vi.useFakeTimers({ now: 1_000_000 });
  const guard = new InputGuard(() => Date.now());
  guard.focus();
  vi.advanceTimersByTime(INPUT_MS);
  const press = (key: string, more: Partial<KeyInput> = {}) => guard.key(keyDown(key, more), true);
  return { guard, press, wait: (ms: number) => void vi.advanceTimersByTime(ms) };
}

const ENTER = { kind: "key", key: "enter" };

describe("UNIT-input-guard", () => {
  test("rule 2: Enter, Space, Escape, ⌘Enter and the digits count, with the digit", () => {
    const { guard, press } = guarded();
    const cases: [string, Partial<KeyInput>, unknown][] = [
      ["Enter", {}, ENTER],
      ["Enter", { shift: true }, ENTER],
      [" ", {}, { kind: "key", key: "space" }],
      ["Escape", {}, { kind: "key", key: "escape" }],
      ["Enter", { meta: true }, { kind: "key", key: "cmd_enter" }],
      ["1", {}, { kind: "key", key: "digit", digit: 1 }],
      ["9", {}, { kind: "key", key: "digit", digit: 9 }],
    ];
    for (const [key, more, evidence] of cases) {
      press(key, more);
      expect(guard.take(), key).toEqual(evidence);
    }
  });

  test("rule 2: other modifiers and repeats do not count; modifier keys alone change nothing", () => {
    const { guard, press } = guarded();
    for (const [key, more] of [
      ["Enter", { control: true }],
      ["Enter", { meta: true, alt: true }],
      [" ", { alt: true }],
      ["1", { meta: true }],
      ["Enter", { isAutoRepeat: true }],
    ] as const) {
      press(key, more);
      expect(guard.take(), `${key} ${JSON.stringify(more)}`).toBeUndefined();
    }
    press("Enter");
    press("Meta");
    press("Shift");
    expect(guard.take()).toEqual(ENTER);
  });

  test("rule 1: only key downs and clicks while the main frame has the focus", () => {
    const { guard, press, wait } = guarded();
    guard.key(keyDown("Enter", { type: "keyUp" }), true);
    guard.key(keyDown("Enter"), false);
    expect(guard.take()).toBeUndefined();
    guard.mouse("mouseDown", "left", true);
    wait(INPUT_MS);
    guard.mouse("mouseUp", "left", true);
    expect(guard.take()).toEqual({ kind: "click" });
    for (const [up, focused] of [
      ["right", true],
      ["left", false],
    ] as const) {
      guard.mouse("mouseDown", "left", true);
      guard.mouse("mouseUp", up, focused);
      expect(guard.take()).toBeUndefined();
    }
    guard.mouse("mouseDown", "left", true);
    wait(INPUT_MS + 1);
    guard.mouse("mouseUp", "left", true);
    expect(guard.take()).toBeUndefined();
    // A key in the review document is not typing either.
    guard.key(keyDown("a"), false);
    press("Enter");
    expect(guard.take()).toEqual(ENTER);
  });

  test("rule 2: the arrows drop the input not used yet, and the Enter after them counts", () => {
    const { guard, press } = guarded();
    press("Enter");
    press("ArrowDown");
    expect(guard.take()).toBeUndefined();
    press("ArrowUp");
    press("Enter");
    expect(guard.take()).toEqual(ENTER);
    press("ArrowLeft");
    press("3");
    expect(guard.take()).toEqual({ kind: "key", key: "digit", digit: 3 });
  });

  test("rule 3: within 1 s of typing, Enter, Space and digits do not count, ⌘Enter does", () => {
    const { guard, press, wait } = guarded();
    press("Enter");
    press("a");
    expect(guard.take(), "typing drops what came before").toBeUndefined();
    for (const key of ["Enter", " ", "2"]) {
      press(key);
      expect(guard.take(), key).toBeUndefined();
    }
    press("Enter", { meta: true });
    expect(guard.take()).toEqual({ kind: "key", key: "cmd_enter" });
    wait(INPUT_MS - 1);
    press("Enter");
    expect(guard.take()).toBeUndefined();
    wait(1);
    press("Enter");
    expect(guard.take()).toEqual(ENTER);
  });

  test("rule 4: input is used once, and only within 1 s", () => {
    const { guard, press, wait } = guarded();
    press("Enter");
    expect(guard.take()).toEqual(ENTER);
    expect(guard.take()).toBeUndefined();
    press("Enter");
    wait(INPUT_MS + 1);
    expect(guard.take()).toBeUndefined();
    press(" ");
    wait(INPUT_MS);
    expect(guard.take()).toEqual({ kind: "key", key: "space" });
  });

  test("rule 6: composing keys count as typing, Enter included", () => {
    const { guard, press, wait } = guarded();
    press("Enter", { isComposing: true });
    expect(guard.take()).toBeUndefined();
    press("Enter");
    expect(guard.take(), "1 s after composing").toBeUndefined();
    wait(INPUT_MS);
    press("Enter");
    expect(guard.take()).toEqual(ENTER);
  });

  test("rule 7: no key counts for 1 s after focus; focus and blur drop input", () => {
    const { guard, press, wait } = guarded();
    press("Enter");
    expect(guard.focus()).toBe(Date.now());
    expect(guard.take()).toBeUndefined();
    press("Enter");
    expect(guard.take()).toBeUndefined();
    // A click is not held back: the activating click never reaches the page.
    guard.mouse("mouseDown", "left", true);
    guard.mouse("mouseUp", "left", true);
    expect(guard.take()).toEqual({ kind: "click" });
    wait(INPUT_MS - 1);
    press("Enter");
    expect(guard.take()).toBeUndefined();
    wait(1);
    press("Enter");
    expect(guard.blur()).toBe(Date.now());
    expect(guard.take()).toBeUndefined();
  });
});
