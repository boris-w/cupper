// The app's icon colour (design 5.7.2 `appIcon`, T252): a UI preference the
// settings page writes and the main process follows with the Dock icon. Its
// value only ever picks a name from this list; it is never a path.

/** The preference's key, as `composer.sendKey` is. */
export const ICON_COLOUR_PREF = "appearance.iconColour";

/** The six colours of `docs/meetings/2026-11-10-app-icon/`, the default first. */
export const ICON_COLOURS = ["latte", "mocha", "sky", "matcha", "ink", "night"] as const;

export type IconColour = (typeof ICON_COLOURS)[number];

/** The bundle's own icon. */
export const DEFAULT_ICON_COLOUR: IconColour = "latte";

/** The preference's value as a colour of the list; anything else is the default. */
export function iconColour(value: unknown): IconColour {
  return ICON_COLOURS.find((colour) => colour === value) ?? DEFAULT_ICON_COLOUR;
}
