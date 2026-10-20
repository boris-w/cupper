import { EventEmitter } from "node:events";

import type { BrowserWindow, WebContents } from "electron";
import { expect, test, vi } from "vitest";

import { FOCUS_CHANGED } from "../../shared/ipc";
import { guardOf, watchInput } from "./index";

test("UNIT-input-guard: a window's keys, clicks and focus reach its guard, on its clock", () => {
  const frame = (id: number) => ({ frameTreeNodeId: id });
  const contents = Object.assign(new EventEmitter(), {
    mainFrame: frame(1),
    focusedFrame: frame(1) as { frameTreeNodeId: number } | null,
    send: vi.fn(),
  });
  const window = Object.assign(new EventEmitter(), { webContents: contents });
  let now = 5000;
  const guard = watchInput(window as unknown as BrowserWindow, () => now);
  expect(guardOf(contents as unknown as WebContents)).toBe(guard);
  window.emit("focus");
  expect(contents.send).toHaveBeenLastCalledWith(FOCUS_CHANGED, true, 5000);
  now += 1000;
  const enter = { type: "keyDown", key: "Enter", isComposing: false, isAutoRepeat: false };
  contents.emit("before-input-event", {}, enter);
  expect(guard.take()).toEqual({ kind: "key", key: "enter" });
  // The review document has the focus: nothing counts.
  contents.focusedFrame = frame(2);
  contents.emit("before-input-event", {}, enter);
  expect(guard.take()).toBeUndefined();
  // Another WebFrameMain object of the main frame is still the main frame.
  contents.focusedFrame = frame(1);
  contents.emit("input-event", {}, { type: "mouseDown", button: "left" });
  contents.emit("input-event", {}, { type: "mouseUp", button: "left" });
  expect(guard.take()).toEqual({ kind: "click" });
  now += 10;
  window.emit("blur");
  expect(contents.send).toHaveBeenLastCalledWith(FOCUS_CHANGED, false, 6010);
});
