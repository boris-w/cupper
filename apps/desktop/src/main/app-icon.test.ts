import { ICON_COLOUR_PREF, iconColour, type IconColour } from "@cupper/shell-api";
import type { NativeImage } from "electron";
import { describe, expect, test } from "vitest";

import { followIconColour } from "./app-icon";
import type { PrefsListener } from "./prefs";

/** Prefs holding `stored`, a Dock that records each icon, and images named by their colour. */
function setup(stored: Record<string, unknown>) {
  const listeners = new Set<PrefsListener>();
  const set: string[] = [];
  const stop = followIconColour({
    prefs: {
      snapshot: () => stored,
      onChange: (listener) => (listeners.add(listener), () => listeners.delete(listener)),
    },
    dock: { setIcon: (image) => void set.push(String(image)) },
    // The default stands for the empty image that brings the bundle's icon back.
    image: (colour: IconColour) =>
      (colour === "latte" ? "bundle" : colour) as unknown as NativeImage,
  });
  const change = (key: string, value: unknown) => listeners.forEach((l) => l(key, value));
  return { set, change, stop };
}

describe("UNIT-app-icon-pref", () => {
  test("a value of the list is that colour; anything else, a path included, is latte", () => {
    expect(iconColour("night")).toBe("night");
    for (const value of [undefined, null, "", "Night", "../../evil", 3, ["sky"]]) {
      expect(iconColour(value)).toBe("latte");
    }
  });

  test("the Dock icon is set once at start and once per change; back to latte is the bundle's icon", () => {
    const { set, change, stop } = setup({ [ICON_COLOUR_PREF]: "matcha" });
    expect(set).toEqual(["matcha"]);
    change(ICON_COLOUR_PREF, "ink");
    // Another key, the same colour again, an unknown value while latte shows: nothing new.
    change("composer.sendKey", "enter");
    change(ICON_COLOUR_PREF, "ink");
    change(ICON_COLOUR_PREF, "latte");
    change(ICON_COLOUR_PREF, "teal");
    expect(set).toEqual(["matcha", "ink", "bundle"]);
    stop();
    change(ICON_COLOUR_PREF, "sky");
    expect(set).toHaveLength(3);
    // No preference yet: the bundle's icon, set before the first window all the same.
    expect(setup({}).set).toEqual(["bundle"]);
  });
});
