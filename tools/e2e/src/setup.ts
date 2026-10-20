// Playwright's global setup: the built app, CLI and fake agent must be there;
// what killed runs left behind goes. The teardown it returns removes the
// defaults Chromium writes for the e2e app outside the test's root (T074).

import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { sweep } from "./fixture";
import { BUILT, DEFAULTS_DOMAIN, ROOTS } from "./paths";

export default async function setup() {
  const missing = Object.values(BUILT).filter((path) => !existsSync(path));
  if (missing.length > 0) {
    throw new Error(
      `not built: ${missing.join(", ")}; \`cargo xtask ci e2e\` builds them all ` +
        "(or `cargo build -p cupper-fake-agent` and `pnpm build:e2e`)",
    );
  }
  const roots = existsSync(ROOTS) ? readdirSync(ROOTS).map((name) => join(ROOTS, name)) : [];
  await sweep(roots);
  return removeDefaults;
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
