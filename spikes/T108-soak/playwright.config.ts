// The soak's case (T108), run with tools/e2e's Playwright and its global setup
// (see run.sh), as T103's are: a plain object, one test at a time. The case
// sets its own time limit from the soak's length.

export default {
  testDir: ".",
  testMatch: "*.spec.ts",
  globalSetup: "../../tools/e2e/src/setup.ts",
  outputDir: "results/test-results",
  workers: 1,
  retries: 0,
  expect: { timeout: 60_000 },
  reporter: [["list"]],
};
