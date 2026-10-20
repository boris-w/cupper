// Each window's InputGuard (design 5.7.2), fed from its webContents and
// focus. A window's bridges write the evidence into commands (design 5.7.8);
// `requestPrivileged` takes it from here too (T134).

import type { BrowserWindow, MouseInputEvent, WebContents } from "electron";

import { FOCUS_CHANGED } from "../../shared/ipc";
import { InputGuard } from "./guard";

export { InputGuard } from "./guard";

const guards = new WeakMap<WebContents, InputGuard>();

/** The guard of a window's webContents, once {@link watchInput} wired it. */
export function guardOf(contents: WebContents): InputGuard | undefined {
  return guards.get(contents);
}

/**
 * Records `window`'s real input, on `now`, and tells its documents when it
 * gains or loses focus, by the same clock (`ShellAPI.window.onFocusChange`).
 */
export function watchInput(window: BrowserWindow, now: () => number): InputGuard {
  const guard = new InputGuard(now);
  const contents = window.webContents;
  guards.set(contents, guard);
  // Frames are compared by their tree node: a WebFrameMain is not one object.
  const mainFrame = () =>
    contents.focusedFrame?.frameTreeNodeId === contents.mainFrame.frameTreeNodeId;
  contents.on("before-input-event", (_event, input) => guard.key(input, mainFrame()));
  contents.on("input-event", (_event, input) => {
    if (input.type === "mouseDown" || input.type === "mouseUp") {
      guard.mouse(input.type, (input as MouseInputEvent).button, mainFrame());
    }
  });
  window.on("focus", () => contents.send(FOCUS_CHANGED, true, guard.focus()));
  window.on("blur", () => contents.send(FOCUS_CHANGED, false, guard.blur()));
  return guard;
}
