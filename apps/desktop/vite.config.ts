// Builds the three entries of the app into `dist/` (design 5.1.3, 5.10.4):
// the main process, the preload and the renderer's main document. Every
// runtime dependency is bundled, so the app declares no `dependencies`.
//
// `vite build --mode <flavor>` picks the build flavor (design 5.7.9). The
// host's environment allowlist comes from the runtime manifest at build time,
// so that the app and the host never keep two copies of it (design 5.6.6).

import { readFileSync } from "node:fs";
import { builtinModules } from "node:module";
import { resolve } from "node:path";

import { defineConfig } from "vite";

import type { Flavor } from "./src/shared/flavor.ts";

// Electron 44.9.0 runs Chromium 152 and Node 24.22.
const CHROME = "chrome152";
const NODE = "node24";

const FLAVORS: Record<Flavor, true> = { release: true, local: true, e2e: true, dev: true };

const at = (path: string) => resolve(import.meta.dirname, path);

/** `env.allow` of `runtime/manifest.json`, as the `define` the main process reads. */
export function hostEnvAllow(): string {
  const manifest = JSON.parse(readFileSync(at("../../runtime/manifest.json"), "utf8")) as {
    env: { allow: string[] };
  };
  return JSON.stringify(manifest.env.allow);
}

export default defineConfig(({ mode }) => {
  if (!Object.hasOwn(FLAVORS, mode)) {
    const flavors = Object.keys(FLAVORS).join(", ");
    throw new Error(`--mode must be a build flavor (${flavors}), not "${mode}"`);
  }
  return {
    define: {
      __CUPPER_FLAVOR__: JSON.stringify(mode),
      __CUPPER_HOST_ENV_ALLOW__: hostEnvAllow(),
    },
    // Build every environment below with one `vite build`.
    builder: {},
    environments: {
      main: {
        consumer: "server",
        keepProcessEnv: true,
        resolve: {
          builtins: ["electron", ...builtinModules, /^node:/],
          noExternal: true,
        },
        build: {
          outDir: at("dist/main"),
          emptyOutDir: true,
          target: NODE,
          sourcemap: true,
          rolldownOptions: {
            input: { index: at("src/main/index.ts") },
            output: { format: "es", entryFileNames: "[name].js" },
          },
        },
      },
      // A sandboxed preload cannot load ES modules or other files: one
      // CommonJS file that requires nothing but `electron`.
      preload: {
        consumer: "client",
        resolve: { builtins: ["electron"] },
        build: {
          outDir: at("dist/preload"),
          emptyOutDir: true,
          target: CHROME,
          sourcemap: true,
          rolldownOptions: {
            input: { index: at("src/preload/index.ts") },
            output: { format: "cjs", entryFileNames: "[name].cjs", codeSplitting: false },
          },
        },
      },
      // The main document, served from app://cupper/. The review document
      // (app://cupper-review/, design 5.7.5) joins in M1b as a second
      // environment with its own root.
      client: {
        build: {
          outDir: at("dist/renderer"),
          emptyOutDir: true,
          target: CHROME,
          rolldownOptions: { input: { index: at("src/renderer/index.html") } },
        },
      },
    },
    // The renderer's root, so that its files land at the top of
    // `dist/renderer/`, the root that app://cupper/ serves.
    root: at("src/renderer"),
  };
});
