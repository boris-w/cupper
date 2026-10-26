// What the baselines were made on (plan 1.8), kept in `baselines/meta.json`.

export interface BaselineMeta {
  chromium: string;
  macos: string;
}

export interface MetaCheck {
  ok: boolean;
  /** What to tell the person running the suite. */
  notes: string[];
}

const major = (version: string) => version.split(".")[0];

/**
 * Chromium has to match exactly: its rendering changes from one version to
 * the next. Another macOS gets a note, and the pixels decide: with the fonts
 * the fixture brings, the CI runner's macOS 15 and the author's macOS 26
 * render all 33 baselines alike (T197). The baselines are the runner's,
 * made again by the `ui-baseline` workflow when pixels differ.
 */
export function compareMeta(baseline: BaselineMeta, current: BaselineMeta): MetaCheck {
  const notes: string[] = [];
  let ok = true;
  if (current.chromium !== baseline.chromium) {
    ok = false;
    notes.push(`Chromium ${current.chromium} is not the baselines' ${baseline.chromium}`);
  }
  if (current.macos !== baseline.macos) {
    const which =
      major(current.macos) === major(baseline.macos) ? "a minor version" : "its major version";
    notes.push(
      `macOS ${current.macos} differs from the baselines' ${baseline.macos} in ${which}; ` +
        "if pixels differ, make the baselines again with the `ui-baseline` workflow",
    );
  }
  return { ok, notes };
}
