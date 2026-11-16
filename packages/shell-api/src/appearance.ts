// The theme and the translucent sidebar (design 5.7.4, 5.7.7; T152): UI
// preferences the settings page writes. The main process follows the theme
// with `nativeTheme.themeSource`, whose change comes back to the windows as
// the appearance they report; the renderer draws the sidebar either way.
// Their values only ever pick from these lists.

/** The theme: the system's, or pinned to one. */
export const THEME_PREF = "appearance.theme";

export const THEME_CHOICES = ["system", "light", "dark"] as const;

export type ThemeChoice = (typeof THEME_CHOICES)[number];

/** The preference's value as a choice of the list; anything else follows the system. */
export function themeChoice(value: unknown): ThemeChoice {
  return THEME_CHOICES.find((choice) => choice === value) ?? "system";
}

/** Whether the sidebar lets the window's material through; on unless turned off. */
export const TRANSLUCENT_SIDEBAR_PREF = "appearance.translucentSidebar";

export const translucentSidebar = (value: unknown): boolean => value !== false;
