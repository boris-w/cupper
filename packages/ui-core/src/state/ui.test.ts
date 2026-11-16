import { describe, expect, test } from "vitest";

import { DEFAULT_PREFS } from "../layout/layout";
import { createUiStore } from "./ui";

const store = () => createUiStore({ prefs: DEFAULT_PREFS, overlayOpen: false });
const visit = (ui: ReturnType<typeof store>, ...ids: string[]) =>
  ids.forEach((id) => ui.getState().navigate({ page: "session", sessionId: id }));
const on = (ui: ReturnType<typeof store>) => {
  const { route } = ui.getState();
  return route.page === "session" ? route.sessionId : route.page;
};

describe("UNIT-ui-store", () => {
  test("⌃Tab goes back and forth between the last two sessions", () => {
    const ui = store();
    visit(ui, "s_1", "s_2", "s_3");
    expect(ui.getState().recent).toEqual(["s_3", "s_2", "s_1"]);
    const all = () => true;
    ui.getState().cycleRecent(1, all);
    expect(on(ui)).toBe("s_2");
    ui.getState().endCycle();
    ui.getState().cycleRecent(1, all);
    expect(on(ui)).toBe("s_3");
    ui.getState().endCycle();
    expect(ui.getState().recent).toEqual(["s_3", "s_2", "s_1"]);
  });

  test("held, ⌃Tab walks further and round; ⌃⇧Tab walks back; only the end counts as a visit", () => {
    const ui = store();
    visit(ui, "s_1", "s_2", "s_3", "s_4");
    const live = (id: string) => id !== "s_2";
    for (const expected of ["s_3", "s_1", "s_4"]) {
      ui.getState().cycleRecent(1, live);
      expect(on(ui)).toBe(expected);
    }
    ui.getState().cycleRecent(-1, live);
    expect([on(ui), ui.getState().recent]).toEqual(["s_1", ["s_4", "s_3", "s_2", "s_1"]]);
    ui.getState().endCycle();
    expect(ui.getState().recent).toEqual(["s_1", "s_4", "s_3", "s_2"]);
  });

  test("from another page ⌃Tab opens the last session; with one session there is nowhere to go", () => {
    const ui = store();
    visit(ui, "s_1");
    ui.getState().cycleRecent(1, () => true);
    expect([on(ui), ui.getState().cycle]).toEqual(["s_1", null]);
    ui.getState().navigate({ page: "new" });
    ui.getState().cycleRecent(1, () => true);
    expect(on(ui)).toBe("s_1");
  });
});
