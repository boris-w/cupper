// The test entry points exist in e2e builds only (design 5.9.9, 5.10.7):
// `window.__cupperTest` and `__cupperGate` (T078), the main process's
// `__cupperE2E` (disconnecting a window) and `__e2eDialogs` (the dialogs'
// stand-ins), `CUPPER_E2E_DIR`, and `CUPPER_E2E_BACKGROUND` (a test's app out
// of the foreground, T195). Built as every other flavor, the main process,
// the preload and the renderer do not name them; built as e2e, they do, so
// that the scan is known to see them.

import { mkdtemp, readdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { createBuilder } from "vite";
import { expect, it } from "vitest";

import type { Flavor } from "./shared/flavor";

const ENTRY_POINTS = [
  "__cupperTest",
  "__cupperGate",
  "__cupperE2E",
  "__e2eDialogs",
  "CUPPER_E2E_DIR",
  "CUPPER_E2E_BACKGROUND",
];

/** The entry points that a build of `flavor` names. */
async function entryPoints(flavor: Flavor): Promise<string[]> {
  const out = await mkdtemp(join(tmpdir(), `cupper-${flavor}-`));
  try {
    const outDir = (name: string) => ({ build: { outDir: join(out, name) } });
    const builder = await createBuilder({
      configFile: join(import.meta.dirname, "../vite.config.ts"),
      mode: flavor,
      logLevel: "silent",
      environments: {
        main: outDir("main"),
        preload: outDir("preload"),
        client: outDir("renderer"),
      },
    });
    await builder.buildApp();
    const files = await readdir(out, { recursive: true, withFileTypes: true });
    const code = files.filter((file) => file.isFile() && /\.(c?js|html)$/.test(file.name));
    const texts = await Promise.all(code.map((file) => readFile(join(file.parentPath, file.name))));
    const all = Buffer.concat(texts).toString();
    return ENTRY_POINTS.filter((name) => all.includes(name));
  } finally {
    await rm(out, { recursive: true, force: true });
  }
}

it("the test entry points: only e2e builds contain them", { timeout: 120_000 }, async () => {
  expect(await entryPoints("e2e")).toEqual(ENTRY_POINTS);
  // Vite refuses `local` as a mode name (`.env.local`); its code is release's.
  for (const flavor of ["release", "dev"] as const) {
    expect(await entryPoints(flavor), flavor).toEqual([]);
  }
});
