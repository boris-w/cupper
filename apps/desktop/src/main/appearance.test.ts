import { THEME_PREF, themeChoice } from "@cupper/shell-api";
import { describe, expect, test } from "vitest";

import { followTheme } from "./appearance";
import type { PrefsListener } from "./prefs";

describe("UI-theme: the theme the settings pin", () => {
  test("a choice of the list is that choice; anything else follows the system", () => {
    expect(themeChoice("dark")).toBe("dark");
    for (const value of [undefined, null, "", "Dark", "auto", 1, ["light"]]) {
      expect(themeChoice(value)).toBe("system");
    }
  });

  test("the native theme takes the preference at start and on each change of it", () => {
    const listeners = new Set<PrefsListener>();
    const theme = { themeSource: "system" };
    const stop = followTheme({
      prefs: {
        snapshot: () => ({ [THEME_PREF]: "dark" }),
        onChange: (listener) => (listeners.add(listener), () => listeners.delete(listener)),
      },
      theme: theme as never,
    });
    const change = (key: string, value: unknown) => listeners.forEach((l) => l(key, value));
    expect(theme.themeSource).toBe("dark");
    change("appearance.iconColour", "ink");
    expect(theme.themeSource, "another key").toBe("dark");
    change(THEME_PREF, "light");
    expect(theme.themeSource).toBe("light");
    change(THEME_PREF, "sepia");
    expect(theme.themeSource, "not a choice").toBe("system");
    stop();
    change(THEME_PREF, "dark");
    expect(theme.themeSource).toBe("system");
  });
});
