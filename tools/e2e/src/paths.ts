// What the suite runs, as `cargo xtask ci e2e` builds it from the current
// commit: the e2e app (`pnpm build:e2e`, its host in `Helpers/`), the e2e CLI
// that `host-bundle --kind e2e` builds next to the host, and the fake agent.

import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = fileURLToPath(new URL("../../../", import.meta.url));
const APP = join(REPO, "apps/desktop/dist/pack/e2e/mac-arm64/Cupper E2E.app");

export const BUILT = {
  app: join(APP, "Contents/MacOS/Cupper E2E"),
  host: join(APP, "Contents/Helpers/CupperHost.app/Contents/MacOS/cupper-host"),
  cli: join(REPO, "target/host-bundle/e2e/cupper"),
  fakeAgent: join(REPO, "target/debug/cupper-fake-agent"),
};

/** Every test's temporary root is a directory here. */
export const ROOTS = join(tmpdir(), "cupper-e2e");

/** Where Chromium writes the e2e app's defaults, outside the test's root (T074). */
export const DEFAULTS_DOMAIN = "dev.cupper.app.e2e";
