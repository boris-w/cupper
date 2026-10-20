// The e2e suite (design 5.9.4, plan 1.8): every test starts its own app, so
// workers stay at 2; more would make focus and cooldown tests flaky on the
// author's machine. Tests tagged `@focus` need the window's focus, which an
// app taking the screen in another worker steals: `test:e2e` runs them after
// the rest, one at a time. `ci pr` runs the `@smoke` tests, `ci e2e` and
// `ci nightly` all of them. `test:e2e` and `test:e2e:one` hold the machine
// lock (`cargo xtask lock`), as `xtask ci` does: runs from two work trees on
// one machine would take each other's focus. They keep the display awake
// (`caffeinate -d`); the global setup refuses a locked screen or a sleeping
// display, where no window can be seen or focused.

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
