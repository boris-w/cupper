import type { BrowserWindowConstructorOptions, WebPreferences } from "electron";

import { isReleaseLike, type Flavor } from "../shared/flavor";

export const MAIN_DOCUMENT_URL = "app://cupper/index.html";

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
