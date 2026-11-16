// The theme the windows follow (design 5.7.7, T152): the system's unless the
// settings pin one. `nativeTheme.themeSource` takes the preference, and
// `nativeTheme`'s update brings the change to every window (shell.ts).

import { THEME_PREF, themeChoice, type ThemeChoice } from "@cupper/shell-api";

import type { PrefsListener } from "./prefs";

export interface ThemeDeps {
  prefs: { snapshot(): Record<string, unknown>; onChange(listener: PrefsListener): () => void };
  theme: { themeSource: ThemeChoice };
}

/** Sets the theme's source now and whenever the preference changes; returns the unsubscribe. */
export function followTheme({ prefs, theme }: ThemeDeps): () => void {
  const apply = (value: unknown) => {
    theme.themeSource = themeChoice(value);
  };
  apply(prefs.snapshot()[THEME_PREF]);
  return prefs.onChange((key, value) => {
    if (key === THEME_PREF) apply(value);
  });
}
