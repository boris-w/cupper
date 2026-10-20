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
 * the next. macOS only has to match in its major version; a minor update gets
 * a note, and the baselines are made again only if pixels differ.
 */
export function compareMeta(baseline: BaselineMeta, current: BaselineMeta): MetaCheck {
  const notes: string[] = [];
  let ok = true;
  if (current.chromium !== baseline.chromium) {
    ok = false;
    notes.push(`Chromium ${current.chromium} is not the baselines' ${baseline.chromium}`);
  }
  if (major(current.macos) !== major(baseline.macos)) {
    ok = false;
    notes.push(`macOS ${current.macos} is not the baselines' major version ${baseline.macos}`);
  } else if (current.macos !== baseline.macos) {
    notes.push(
      `macOS ${current.macos} differs from the baselines' ${baseline.macos} in a minor version; ` +
        "if pixels differ, make the baselines again with `pnpm ui:baseline`",
    );
  }
  return { ok, notes };
}
