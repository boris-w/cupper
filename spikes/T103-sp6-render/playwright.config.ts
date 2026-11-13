// SP-6's cases (T103), run with tools/e2e's Playwright and its global setup
// (see run.sh): a plain object, so that nothing here resolves a second copy
// of Playwright from outside the workspace. One test at a time, as measuring
// wants the runner to itself.

export default {
  testDir: ".",
  testMatch: "*.spec.ts",
  globalSetup: "../../tools/e2e/src/setup.ts",
  outputDir: "results/test-results",
  workers: 1,
  retries: 0,
  timeout: 40 * 60_000,
  expect: { timeout: 60_000 },
  reporter: [["list"]],
};
