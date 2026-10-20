import type { ShellAPI } from "@cupper/shell-api";
import { describe, expect, test } from "vitest";

import {
  computeLayout,
  DEFAULT_PREFS,
  dragResize,
  keyResize,
  ratioOf,
  type LayoutPrefs,
} from "./layout";
import { LayoutStore, PREF_KEYS, readPrefs } from "./store";

const view = (width: number, height = 820) => ({ width, height });
const open: LayoutPrefs = { ...DEFAULT_PREFS, sidePanelOpen: true };

describe("UNIT-ui-shell-resize", () => {
  test("the default window docks a 288 sidebar and gives the side panel its default width", () => {
    const layout = computeLayout(view(1280), open, false);
    expect(layout.sidebar).toMatchObject({ mode: "docked", visible: true, width: 288 });
    expect(layout.main).toBe(992);
    // max(320, min(820 × 1.6, 992 − 500), min(640, 992 − 352)) = 640, the maximum.
    expect(layout.sidePanel).toMatchObject({ visible: true, width: 640, min: 320, max: 640 });
  });

  test("the sidebar stays 240–520 wide and leaves the main area 240", () => {
    const wide = { ...DEFAULT_PREFS, sidebarWidth: 900 };
    expect(computeLayout(view(1280), wide, false).sidebar.width).toBe(520);
    expect(computeLayout(view(700), wide, false).sidebar.width).toBe(700 - 240);
    expect(computeLayout(view(1280), { ...wide, sidebarWidth: 10 }, false).sidebar.width).toBe(240);
  });

  test("a window narrower than 768 floats the sidebar, which follows the overlay state only", () => {
    const narrow = computeLayout(view(767), DEFAULT_PREFS, false);
    expect(narrow.sidebar).toMatchObject({ mode: "overlay", visible: false });
    expect(narrow.main).toBe(767);
    const shown = computeLayout(view(767), { ...DEFAULT_PREFS, sidebarOpen: false }, true);
    expect(shown.sidebar).toMatchObject({ mode: "overlay", visible: true });
    expect(shown.main).toBe(767);
    expect(computeLayout(view(768), DEFAULT_PREFS, true).sidebar.mode).toBe("docked");
  });

  test("the side panel collapses while 352 + 320 do not fit, and comes back when they do", () => {
    // 288 + 352 + 320 = 960.
    const collapsed = computeLayout(view(959), open, false);
    expect(collapsed.sidePanel).toMatchObject({ visible: false, collapsed: true });
    const restored = computeLayout(view(960), open, false);
    expect(restored.sidePanel).toMatchObject({ visible: true, collapsed: false, width: 320 });
    // Hiding the sidebar makes room as well; a closed panel is not collapsed.
    expect(computeLayout(view(959), { ...open, sidebarOpen: false }, false).sidePanel.visible).toBe(
      true,
    );
    expect(computeLayout(view(959), DEFAULT_PREFS, false).sidePanel.collapsed).toBe(false);
  });

  test("the side panel keeps its ratio between minimum and maximum as the window changes", () => {
    const first = computeLayout(view(1280), open, false).sidePanel;
    const ratio = ratioOf(480, first);
    expect(ratio).toBe(0.5);
    const wider = computeLayout(view(1600), { ...open, sidePanelRatio: ratio }, false).sidePanel;
    expect(wider.width).toBe(Math.round((wider.min + wider.max) / 2));
    expect(ratioOf(10, first)).toBe(0);
    expect(ratioOf(10_000, first)).toBe(1);
  });

  test("keys move a handle by a step, Home and End jump to the bounds", () => {
    const bounds = { min: 320, max: 640 };
    expect(keyResize("ArrowLeft", 400, bounds, "ArrowLeft")).toBe(416);
    expect(keyResize("ArrowRight", 400, bounds, "ArrowLeft")).toBe(384);
    expect(keyResize("ArrowRight", 636, bounds, "ArrowRight")).toBe(640);
    expect(keyResize("Home", 400, bounds, "ArrowLeft")).toBe(320);
    expect(keyResize("End", 400, bounds, "ArrowLeft")).toBe(640);
    expect(keyResize("Enter", 400, bounds, "ArrowLeft")).toBeUndefined();
  });

  test("a drag widens toward the panel's free side and stops at the bounds", () => {
    const bounds = { min: 240, max: 520 };
    expect(dragResize(288, 300, 340, bounds, "right")).toBe(328);
    expect(dragResize(400, 900, 860, { min: 320, max: 640 }, "left")).toBe(440);
    expect(dragResize(288, 300, 0, bounds, "right")).toBe(240);
  });

  test("the store reads, saves and follows the layout preferences", async () => {
    const saved: [string, unknown][] = [];
    let changed: ((key: string, value: unknown) => void) | undefined;
    const prefs: ShellAPI["prefs"] = {
      set: (key, value) => Promise.resolve(void saved.push([key, value])),
      onChange: (cb) => ((changed = cb), () => undefined),
    };
    const snapshot = { [PREF_KEYS.sidebarWidth]: "wide", [PREF_KEYS.sidePanelRatio]: 0.25 };
    expect(readPrefs(snapshot)).toEqual({ ...DEFAULT_PREFS, sidePanelRatio: 0.25 });

    const store = new LayoutStore(prefs, snapshot);
    store.set("sidebarWidth", 300, false);
    store.set("sidebarWidth", 310);
    store.toggleSidebar(false);
    store.toggleSidebar(true);
    await Promise.resolve();
    expect(saved).toEqual([
      [PREF_KEYS.sidebarWidth, 310],
      [PREF_KEYS.sidebarOpen, false],
    ]);
    expect(store.getState()).toMatchObject({ overlayOpen: true, prefs: { sidebarOpen: false } });

    changed?.(PREF_KEYS.sidePanelOpen, true);
    changed?.(PREF_KEYS.sidePanelRatio, 7);
    expect(store.getState().prefs).toMatchObject({ sidePanelOpen: true, sidePanelRatio: 0.25 });
  });
});
