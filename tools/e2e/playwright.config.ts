// The e2e suite (design 5.9.4, plan 1.8): every test starts its own app, so
// workers stay at 2; more would make focus and cooldown tests flaky on the
// author's machine. Tests tagged `@focus` need the window's focus, which an
// app taking the screen in another worker steals: `test:e2e` runs them after
// the rest, one at a time. `ci pr` runs the `@smoke` tests, `ci e2e` and
// `ci nightly` all of them.

import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "src",
  testMatch: "*.spec.ts",
  globalSetup: "./src/setup.ts",
  outputDir: "dist/test-results",
  workers: 2,
  fullyParallel: true,
  forbidOnly: true,
  retries: 0,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: [["list"]],
});
