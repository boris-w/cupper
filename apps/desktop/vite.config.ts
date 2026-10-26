// Builds the three entries of the app into `dist/` (design 5.1.3, 5.10.4):
// the main process, the preload and the renderer's main document. Every
// runtime dependency is bundled, so the app declares no `dependencies`.
//
// `vite build --mode <flavor>` picks the build flavor (design 5.7.9). The
// host's environment allowlist comes from the runtime manifest at build time,
// so that the app and the host never keep two copies of it (design 5.6.6).

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { builtinModules } from "node:module";
import { relative, resolve } from "node:path";

import tailwindcss from "@tailwindcss/vite";
import { defineConfig, type Plugin } from "vite";

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

/**
 * Writes the modules each environment's outputs hold, as paths relative to
 * the repository, to `dist/metafile/<environment>.json`: the license gate
 * reads them (T006, design 5.1.6). No package of the app takes that
 * directory up.
 */
function metafile(): Plugin {
  const repo = at("../..");
  // A virtual module (`\0…`) is the bundler's own and has no path.
  const shown = (id: string) => (id.startsWith("\0") ? id : relative(repo, id));
  return {
    name: "cupper-metafile",
    generateBundle(_options, bundle) {
      const outputs = Object.values(bundle).flatMap((output) =>
        output.type === "chunk"
          ? [{ file: output.fileName, modules: output.moduleIds.map(shown) }]
          : [],
      );
      mkdirSync(at("dist/metafile"), { recursive: true });
      const file = at(`dist/metafile/${this.environment.name}.json`);
      writeFileSync(file, `${JSON.stringify(outputs, null, 2)}\n`);
    },
  };
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
    // The renderer's styles: Tailwind over the tokens of `@cupper/ui-core`.
    plugins: [tailwindcss(), metafile()],
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
            // Electron's own module at runtime, not the npm package that finds its binary.
            external: ["electron"],
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
