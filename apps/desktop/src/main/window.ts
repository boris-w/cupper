import type { EventEmitter } from "node:events";

import type { BrowserWindowConstructorOptions, WebPreferences } from "electron";

import { isReleaseLike, type Flavor } from "../shared/flavor";

export const MAIN_DOCUMENT_URL = "app://cupper/index.html";
/** The review document (design 5.7.5), framed by the main document. */
export const REVIEW_DOCUMENT_URL = "app://cupper-review/review.html";

/** The hardened webPreferences of every window (design 5.7.8). */
export function webPreferences(flavor: Flavor, preload: string): WebPreferences {
  return {
    preload,
    sandbox: true,
    contextIsolation: true,
    nodeIntegration: false,
    nodeIntegrationInWorker: false,
    nodeIntegrationInSubFrames: false,
    webviewTag: false,
    webSecurity: true,
    allowRunningInsecureContent: false,
    experimentalFeatures: false,
    navigateOnDragDrop: false,
    // Release builds have no DevTools (design 5.7.8).
    devTools: !isReleaseLike(flavor),
  };
}

/**
 * The main window (design 5.7.4, after Codex). The content runs up to the top
 * edge, and the traffic lights sit at x = 16, centred in the 44 px title bar:
 * (44 - 14) / 2 = 15. The window stays hidden until its first paint, so it
 * never flashes white. `acceptFirstMouse` keeps its default, false: the click
 * that activates the window does not reach the page.
 */
export function mainWindowOptions(
  flavor: Flavor,
  preload: string,
): BrowserWindowConstructorOptions {
  return {
    width: 1280,
    height: 820,
    minWidth: 480,
    minHeight: 600,
    titleBarStyle: "hiddenInset",
    trafficLightPosition: { x: 16, y: 15 },
    show: false,
    webPreferences: webPreferences(flavor, preload),
  };
}

/** The window events Electron 44 follows itself to tell the page whether the window is visible. */
const VISIBILITY_EVENTS = ["show", "hide", "minimize", "maximize", "restore"] as const;

/**
 * Once `window` is destroyed, macOS may still report a change of its
 * occlusion, which Electron emits as `show` or `hide`. Electron's own
 * listener for these events then asks the destroyed window whether it is
 * visible, and throws "Object has been destroyed" (T260). So when the window
 * closes, the listeners of these events go: nothing needs them after it.
 * Electron's main branch no longer has that listener
 * (https://github.com/electron/electron/pull/53971); 44 does.
 */
export function dropVisibilityListenersOnClose(window: EventEmitter): void {
  window.once("closed", () =>
    VISIBILITY_EVENTS.forEach((event) => window.removeAllListeners(event)),
  );
}
