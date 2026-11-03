// The self-use checks (T160), which `cargo xtask ci selfuse` runs on a
// GitHub-hosted runner once it has built the apps: a file at a time, since
// each starts apps on the same data directories.

import { defineConfig } from "vitest/config";

export default defineConfig({
  test: { include: ["selfuse/**/*.test.ts"], fileParallelism: false, testTimeout: 300_000 },
});
