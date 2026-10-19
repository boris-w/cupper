/**
 * Test IDs, `<layer>-<scope>-<name>` (design 5.9.10): finding them in text and
 * telling which ID a test belongs to. The rules match `crates/xtask/src/ci/ids.rs`,
 * which attributes test cases to IDs when it writes a CI summary and leaves out
 * the manual `MAN-` layer, which has no tests.
 */

/** The layers of design 5.9.10, `MAN` for the manual checks. */
export const LAYERS = [
  "UNIT",
  "PROP",
  "GOLD-A",
  "GOLD-B",
  "IT",
  "E2E",
  "UI",
  "SMOKE-SIGNED",
  "COMPAT",
  "PERF",
  "MAN",
] as const;

const LAYER = `(?:${LAYERS.join("|")})`;
const IN_TEXT = new RegExp(`(?<![A-Za-z0-9_-])${LAYER}-[A-Za-z0-9-]*[A-Za-z0-9]`, "g");
const WHOLE = new RegExp(`^${LAYER}(?:-[A-Za-z0-9]+)+$`);

/** Every ID-shaped token in `text`, such as `UNIT-ci-lock` or `GOLD-A-claude-s01`. */
export function idsIn(text: string): string[] {
  return [...text.matchAll(IN_TEXT)].map((match) => match[0]);
}

/** A layer followed by `-`-separated segments of letters and digits. */
export function isId(text: string): boolean {
  return WHOLE.test(text);
}

/**
 * The longest ID that starts a `::`-separated segment of `testName`, either in
 * snake case and followed by `_` or the end (Rust: `unit_ci_lock_...`), or
 * verbatim and followed by anything that cannot continue an ID (JS and
 * Playwright titles: `UNIT-ci-lock waits`).
 */
export function owner(ids: Iterable<string>, testName: string): string | undefined {
  const segments = testName.split("::");
  let best: string | undefined;
  for (const id of ids) {
    const snake = id.toLowerCase().replaceAll("-", "_");
    const owns = segments.some(
      (segment) =>
        (segment.startsWith(snake) && /^(?:_|$)/.test(segment.slice(snake.length))) ||
        (segment.startsWith(id) && !/^[A-Za-z0-9_-]/.test(segment.slice(id.length))),
    );
    if (owns && (best === undefined || id.length > best.length)) {
      best = id;
    }
  }
  return best;
}
