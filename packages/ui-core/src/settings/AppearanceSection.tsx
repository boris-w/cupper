// Appearance (design 5.7.4): the app's icon colour, cupper's own (T252). It
// is a UI preference the main process follows with the Dock icon of the
// running app; the Finder, Launchpad and notifications keep the bundle's.
// The small icons are the renderer's own assets, loaded from `app://`.

import { ICON_COLOUR_PREF, ICON_COLOURS, iconColour, type IconColour } from "@cupper/shell-api";

import { useShell, useT } from "../app/providers";
import { usePref } from "../composer/send";
import ink from "./icons/ink.png";
import latte from "./icons/latte.png";
import matcha from "./icons/matcha.png";
import mocha from "./icons/mocha.png";
import night from "./icons/night.png";
import sky from "./icons/sky.png";
import { Row, Section } from "./parts";

const ICONS: Record<IconColour, string> = { latte, mocha, sky, matcha, ink, night };

export function AppearanceSection() {
  const t = useT();
  const { api } = useShell();
  const chosen = iconColour(usePref(ICON_COLOUR_PREF));
  return (
    <Section title={t("settings:icon")}>
      <Row label={t("settings:icon_colour")} description={t("settings:icon_colour_detail")}>
        {/* Native radios: one tab stop, the arrow keys move between them. */}
        <div
          role="radiogroup"
          aria-label={t("settings:icon_colour")}
          className="flex shrink-0 gap-1"
        >
          {ICON_COLOURS.map((colour) => (
            <label
              key={colour}
              className="rounded-xl p-0.5 ring-accent has-checked:ring-2 has-focus-visible:ring-2 has-focus-visible:ring-ring"
            >
              <input
                type="radio"
                name="icon-colour"
                value={colour}
                checked={colour === chosen}
                onChange={() => void api.prefs.set(ICON_COLOUR_PREF, colour)}
                className="sr-only"
              />
              {/* The label, and so the radio, takes its name from the picture. */}
              <img
                src={ICONS[colour]}
                alt={t(`settings:icon_colour_${colour}`)}
                className="size-10"
              />
            </label>
          ))}
        </div>
      </Row>
    </Section>
  );
}
