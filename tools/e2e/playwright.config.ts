// The e2e suite (design 5.9.4, plan 1.8): every test starts its own app, and
// its app, host and agent take about a core while they start. So workers are
// the cores but one, for Playwright and the system, and 2 at most: a runner's
// 3 cores take 2 (T197), and more on the author's machine made focus and
// cooldown tests flaky (T090). Tests tagged `@focus` need the window's focus,
// which an app taking the screen in another worker steals: `test:e2e` runs
// them after the rest, one at a time. `ci pr` runs the `@smoke` tests (in the
// same two phases), `ci e2e` and `ci nightly` all of them. `test:e2e` and
// `test:e2e:one` hold the machine lock (`cargo xtask lock`), as `xtask ci`
// does: runs from two work trees on one machine would take each other's
// focus. They keep the display awake (`caffeinate -d`); the global setup
// refuses a locked screen or a sleeping display, where no window can be seen
// or focused.

import { availableParallelism } from "node:os";

import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "src",
  testMatch: "*.spec.ts",
  globalSetup: "./src/setup.ts",
  outputDir: "dist/test-results",
  workers: Math.max(1, Math.min(2, availableParallelism() - 1)),
  fullyParallel: true,
  forbidOnly: true,
  retries: 0,
  // Generous: other runs on the author's machine slow the apps down; a
  // passing test does not wait for these.
  timeout: 90_000,
  expect: { timeout: 20_000 },
  reporter: [["list"]],
});
