import { defineConfig } from "vitest/config";

import { hostEnvAllow } from "./vite.config";

export default defineConfig({
  define: { __CUPPER_HOST_ENV_ALLOW__: hostEnvAllow() },
  // Only the sources: `tsc --build` also compiles the tests into `dist/`.
  test: { include: ["src/**/*.test.ts"] },
});
