// Builds the screenshot page into `dist/page/`; the suite serves it under the
// main document's CSP (see `src/serve.ts`).

import { resolve } from "node:path";

import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
  root: resolve(import.meta.dirname, "src/page"),
  plugins: [tailwindcss()],
  logLevel: "warn",
  build: { outDir: resolve(import.meta.dirname, "dist/page"), emptyOutDir: true },
});
