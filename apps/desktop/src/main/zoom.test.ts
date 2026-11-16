import { describe, expect, test } from "vitest";

import { lightsAt, zoom, type Zoomable } from "./zoom";

/** A window whose zoom factor follows its level, as Chromium's does: 1.2 to the level. */
function window(level = 0) {
  const placed: { x: number; y: number }[] = [];
  const target: Zoomable = {
    webContents: { zoomLevel: level, getZoomFactor: () => 1.2 ** target.webContents.zoomLevel },
    setWindowButtonPosition: (position) => void placed.push(position),
  };
  return { target, placed };
}

describe("UI-layout: the traffic lights at each zoom", () => {
  test("centred in the 44 high title bar as it zooms: (44 × zoom − 14) / 2", () => {
    expect(lightsAt(1)).toEqual({ x: 16, y: 15 });
    expect(lightsAt(1.2)).toEqual({ x: 16, y: 19 });
    expect(lightsAt(1.44)).toEqual({ x: 16, y: 25 });
    expect(lightsAt(1 / 1.2)).toEqual({ x: 16, y: 11 });
  });

  test("each step is half a level, as Electron's roles; the lights follow", () => {
    const { target, placed } = window();
    zoom(target, "in");
    zoom(target, "in");
    expect(target.webContents.zoomLevel).toBe(1);
    zoom(target, "out");
    expect(target.webContents.zoomLevel).toBe(0.5);
    zoom(target, "reset");
    expect(target.webContents.zoomLevel).toBe(0);
    expect(placed.map(({ y }) => y)).toEqual([17, 19, 17, 15]);
  });
});
