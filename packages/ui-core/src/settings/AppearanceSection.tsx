// Appearance (design 5.7.4, 5.7.7): the theme, the system's or pinned, and
// the translucent sidebar (T152); the app's icon colour, cupper's own (T252).
// They are UI preferences: the main process follows the theme with the
// windows' appearance and the icon colour with the Dock icon of the running
// app; the Finder, Launchpad and notifications keep the bundle's icon. The
// small icons are the renderer's own assets, loaded from `app://`.

import {
  ICON_COLOUR_PREF,
  ICON_COLOURS,
  iconColour,
  THEME_CHOICES,
  THEME_PREF,
  themeChoice,
  TRANSLUCENT_SIDEBAR_PREF,
  translucentSidebar,
  type IconColour,
} from "@cupper/shell-api";

import { useShell, useT } from "../app/providers";
import { usePref } from "../composer/send";
import ink from "./icons/ink.png";
import latte from "./icons/latte.png";
import matcha from "./icons/matcha.png";
import mocha from "./icons/mocha.png";
import night from "./icons/night.png";
import sky from "./icons/sky.png";
import { Row, Section, SwitchRow } from "./parts";

const ICONS: Record<IconColour, string> = { latte, mocha, sky, matcha, ink, night };

export function AppearanceSection() {
  const t = useT();
  const { api } = useShell();
  const chosen = iconColour(usePref(ICON_COLOUR_PREF));
  const theme = themeChoice(usePref(THEME_PREF));
  const translucent = translucentSidebar(usePref(TRANSLUCENT_SIDEBAR_PREF));
  return (
    <>
      <Section title={t("settings:look")}>
        <Row label={t("settings:theme")}>
          {/* A segmented control of native radios: one tab stop, the arrow keys move. */}
          <div
            role="radiogroup"
            aria-label={t("settings:theme")}
            className="flex shrink-0 rounded-lg bg-btn-secondary p-0.5"
          >
            {THEME_CHOICES.map((choice) => (
              <label
                key={choice}
                className="rounded-md px-2.5 py-1 text-fg-secondary has-checked:bg-surface-elevated has-checked:text-fg has-checked:shadow-card has-focus-visible:ring-2 has-focus-visible:ring-ring"
              >
                <input
                  type="radio"
                  name="theme"
                  value={choice}
                  checked={choice === theme}
                  onChange={() => void api.prefs.set(THEME_PREF, choice)}
                  className="sr-only"
                />
                {t(`settings:theme_${choice}`)}
              </label>
            ))}
          </div>
        </Row>
        <SwitchRow
          label={t("settings:translucent_sidebar")}
          description={t("settings:translucent_sidebar_detail")}
          checked={translucent}
          onChange={(on) => void api.prefs.set(TRANSLUCENT_SIDEBAR_PREF, on)}
        />
      </Section>
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
    </>
  );
}
