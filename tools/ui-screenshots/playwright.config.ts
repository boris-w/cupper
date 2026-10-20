// The screenshot regression (design 5.9.1, plan 1.8). Baselines come only from
// `pnpm ui:baseline`; a plain run never writes one, and fails on a missing one.

import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "src",
  testMatch: "*.spec.ts",
  globalSetup: "./src/setup.ts",
  snapshotPathTemplate: "{testDir}/../baselines/{arg}{ext}",
  outputDir: "dist/test-results",
  updateSnapshots: process.env.CUPPER_UI_BASELINE ? "changed" : "none",
  forbidOnly: true,
  workers: 1,
  reporter: [["list"]],
  expect: { toHaveScreenshot: { animations: "disabled", caret: "hide", scale: "device" } },
  use: {
    browserName: "chromium",
    locale: "zh-CN",
    timezoneId: "Asia/Shanghai",
    viewport: { width: 1280, height: 820 },
    deviceScaleFactor: 2,
    colorScheme: "light",
    reducedMotion: "reduce",
  },
});
