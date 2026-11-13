// appIcon (design 5.7.2, T252): the Dock icon in the colour the preference
// names, set before the first window and again on each change of it. Only
// the running app's Dock icon follows: the Finder and Launchpad show the
// bundle's icon, notifications the host bundle's.

import { ICON_COLOUR_PREF, iconColour, type IconColour } from "@cupper/shell-api";
import type { NativeImage } from "electron";

import type { PrefsListener } from "./prefs";

export interface IconDeps {
  prefs: { snapshot(): Record<string, unknown>; onChange(listener: PrefsListener): () => void };
  dock: { setIcon(image: NativeImage): void };
  /** The image of a colour: for the default, an empty one, which brings the bundle's icon back. */
  image(colour: IconColour): NativeImage;
}

/** Sets the Dock icon now and whenever the preference changes; returns the unsubscribe. */
export function followIconColour({ prefs, dock, image }: IconDeps): () => void {
  let shown: IconColour | undefined;
  const show = (value: unknown) => {
    const colour = iconColour(value);
    if (colour !== shown) {
      shown = colour;
      dock.setIcon(image(colour));
    }
  };
  show(prefs.snapshot()[ICON_COLOUR_PREF]);
  return prefs.onChange((key, value) => {
    if (key === ICON_COLOUR_PREF) show(value);
  });
}
