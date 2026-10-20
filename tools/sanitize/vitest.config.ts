import { defineConfig } from "vitest/config";

export default defineConfig({
  // Only the sources: `tsc --build` also compiles the tests into `dist/`.
  test: { include: ["src/**/*.test.ts"] },
});
