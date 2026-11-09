// release-config (T161), which `cargo xtask ci release-config` runs on the
// release build it made.

import { defineConfig } from "vitest/config";

export default defineConfig({
  // The test prints what it only reports: Info.plist's usage descriptions.
  test: { include: ["release/**/*.test.ts"], testTimeout: 60_000, silent: false },
});
