// Playwright's global setup: the built app, CLI and fake agent must be there,
// and the screen must show the app's windows; what killed runs left behind
// goes. The teardown it returns removes the defaults Chromium writes for the
// e2e app outside the test's root (T074).

import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, realpathSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { sweep } from "./fixture";
import { BUILT, DEFAULTS_DOMAIN, ROOTS } from "./paths";

export default async function setup() {
  // `/usr/bin/git` is a stub that asks to install the command line tools
  // unless Xcode or they are selected; the hosts run it.
  try {
    execFileSync("xcode-select", ["-p"], { stdio: "ignore" });
  } catch {
    throw new Error("no Xcode or command line tools: `/usr/bin/git` would not run");
  }
  const unusable = screenUnusable();
  if (unusable) {
    throw new Error(
      `${unusable}: the app's windows can be neither seen nor focused, so the \`@focus\` ` +
        "tests cannot run; unlock the screen or wake the display and run again",
    );
  }
  const missing = Object.values(BUILT).filter((path) => !existsSync(path));
  if (missing.length > 0) {
    throw new Error(
      `not built: ${missing.join(", ")}; \`cargo xtask ci e2e\` builds them all ` +
        "(or `cargo build -p cupper-fake-agent` and `pnpm build:e2e`)",
    );
  }
  const names = existsSync(ROOTS) ? readdirSync(ROOTS) : [];
  // As the processes name them: `$TMPDIR` is a link.
  await sweep(names.map((name) => realpathSync(join(ROOTS, name))));
  return removeDefaults;
}

/**
 * Why the screen cannot show the app's windows, if it cannot (T093): the
 * screen locked or the display asleep, as CoreGraphics tells them, or this
 * login session not the one on the console. A window there is never focused,
 * and its timers are held back, so real input fails as if the tests did.
 */
function screenUnusable(): string | undefined {
  const script = `
    ObjC.import("CoreGraphics");
    const ref = $.CGSessionCopyCurrentDictionary();
    const session = ref ? ObjC.deepUnwrap(ObjC.castRefToObject(ref)) : {};
    JSON.stringify({
      locked: session.CGSSessionScreenIsLocked === true,
      asleep: Boolean($.CGDisplayIsAsleep($.CGMainDisplayID())),
      away: session.kCGSSessionOnConsoleKey !== true,
    });`;
  const out = execFileSync("osascript", ["-l", "JavaScript", "-e", script], { encoding: "utf8" });
  const state = JSON.parse(out) as { locked: boolean; asleep: boolean; away: boolean };
  const why = [
    state.locked && "the screen is locked",
    state.asleep && "the display is asleep",
    state.away && "this login session is not on the console",
  ].filter(Boolean);
  return why.length > 0 ? why.join(", ") : undefined;
}

function removeDefaults() {
  try {
    // Through cfprefsd, so that its cache forgets them too.
    execFileSync("defaults", ["delete", DEFAULTS_DOMAIN], { stdio: "ignore" });
  } catch {
    // Nothing was written.
  }
  rmSync(join(homedir(), "Library/Preferences", `${DEFAULTS_DOMAIN}.plist`), { force: true });
}
