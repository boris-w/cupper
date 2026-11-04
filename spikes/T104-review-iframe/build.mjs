// Builds T104: the main process and the preload for Electron (CommonJS), the
// main document, and the review document with its worker (ESM, split).
import { copyFileSync, mkdirSync, rmSync } from "node:fs";

import { build } from "esbuild";

const shellApi = new URL("../../packages/shell-api/src/index.ts", import.meta.url).pathname;
const common = { bundle: true, logLevel: "warning", alias: { "@cupper/shell-api": shellApi } };
const browser = {
  ...common,
  platform: "browser",
  format: "esm",
  jsx: "automatic",
  minify: true,
  define: { "process.env.NODE_ENV": '"production"' },
};
rmSync("dist", { recursive: true, force: true });
await build({
  ...common,
  entryPoints: ["src/main.ts", "src/preload.ts"],
  outdir: "dist",
  outExtension: { ".js": ".cjs" },
  platform: "node",
  format: "cjs",
  external: ["electron"],
});
await build({ ...browser, entryPoints: { main: "src/main-doc.ts" }, outdir: "dist/cupper" });
await build({
  ...browser,
  entryPoints: { review: "src/review.tsx", worker: "src/worker.ts" },
  outdir: "dist/review",
  splitting: true,
});
mkdirSync("dist/cupper", { recursive: true });
copyFileSync("src/cupper.html", "dist/cupper/index.html");
copyFileSync("src/review.html", "dist/review/index.html");
