// Builds the screenshot page and the review document into `dist/page/`; the
// suite serves them from two origins under their CSPs (see `src/serve.ts`).

import { resolve } from "node:path";

import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

const page = (file: string) => resolve(import.meta.dirname, "src/page", file);

export default defineConfig({
  root: resolve(import.meta.dirname, "src/page"),
  plugins: [tailwindcss()],
  logLevel: "warn",
  build: {
    outDir: resolve(import.meta.dirname, "dist/page"),
    emptyOutDir: true,
    rolldownOptions: { input: [page("index.html"), page("review.html")] },
  },
  worker: { format: "es" },
});
