// The window's zoom (design 5.7.4, 5.7.7), from the View menu. Electron's
// roles zoom the page but leave the traffic lights where they were, so the
// menu does it itself, as the roles do: half a level a step, or back to 0.
// After each step the traffic lights are centred again in the title bar,
// which zooms with the page: y = (44 × zoom − 14) / 2, x = 16 (Codex).

export type ZoomStep = "reset" | "in" | "out";

const TITLE_BAR_H = 44;
const LIGHTS_H = 14;
const LIGHTS_X = 16;

/** Where the traffic lights go at zoom factor `factor`. */
export const lightsAt = (factor: number) => ({
  x: LIGHTS_X,
  y: Math.round((TITLE_BAR_H * factor - LIGHTS_H) / 2),
});

export interface Zoomable {
  webContents: { zoomLevel: number; getZoomFactor(): number };
  setWindowButtonPosition(position: { x: number; y: number }): void;
}

/** Zooms `window` one step, then puts its traffic lights back in the middle of the title bar. */
export function zoom(window: Zoomable, step: ZoomStep): void {
  const contents = window.webContents;
  contents.zoomLevel = step === "reset" ? 0 : contents.zoomLevel + (step === "in" ? 0.5 : -0.5);
  placeLights(window);
}

/** The traffic lights at the window's zoom: after a step, and at load, as the zoom stays with the page. */
export function placeLights(window: Zoomable): void {
  window.setWindowButtonPosition(lightsAt(window.webContents.getZoomFactor()));
}
