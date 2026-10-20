import { defineConfig } from "vitest/config";

export default defineConfig({
  // Only the sources: `tsc --build` also compiles the tests into `dist/`.
  // Component tests pick jsdom in their first line.
  test: { include: ["src/**/*.test.{ts,tsx}"] },
});
