/**
 * Command-line switches a release build starts with (design 5.7.8). macOS
 * starts an app without any, so the list is empty: `--inspect`,
 * `--remote-debugging-port`, `--js-flags`, `--no-sandbox` and the rest all
 * make it exit. Names are written without the leading dashes.
 */
export const ALLOWED_SWITCHES: ReadonlySet<string> = new Set<string>();

/**
 * The switches in `args` (the arguments after the executable) that are not
 * allowed. Chromium also reads `-name`, so anything starting with a dash
 * counts, `--` included.
 */
export function unknownSwitches(args: readonly string[]): string[] {
  return args.filter((arg) => {
    if (!arg.startsWith("-")) {
      return false;
    }
    const name = arg.replace(/^--?/, "").split("=", 1)[0] ?? "";
    return !ALLOWED_SWITCHES.has(name);
  });
}

/** Exits with status 1 when `args` has a switch that is not allowed. */
export function enforceSwitchAllowlist(
  args: readonly string[],
  exit: (code: number) => void,
  report: (message: string) => void,
) {
  const unknown = unknownSwitches(args);
  if (unknown.length > 0) {
    report(`cupper: refusing to start with unknown switches: ${unknown.join(" ")}`);
    exit(1);
  }
}
