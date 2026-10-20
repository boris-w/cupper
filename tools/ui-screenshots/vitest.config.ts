import { defineConfig } from "vitest/config";

export default defineConfig({
  // The Playwright suite is `*.spec.ts`; `tsc --build` also compiles into `dist/`.
  test: { include: ["src/**/*.test.ts"] },
});
