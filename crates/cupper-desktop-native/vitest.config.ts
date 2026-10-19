import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Only the sources: `tsc --build` also compiles the tests into `dist/`.
    include: ["ts/**/*.test.ts"],
    // A process of its own loads the extension and starts the fixture, so
    // the fixture's launcher is the process that connects (design 5.8.1).
    pool: "forks",
  },
});
