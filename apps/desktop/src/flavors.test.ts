// Built as every flavor but e2e, the main process, the preload and the
// renderer name none of the test entry points; built as e2e, they name every
// one, so that the scan is known to see them.

import { mkdtemp, readdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { createBuilder } from "vite";
import { expect, it } from "vitest";

import { TEST_ENTRY_POINTS } from "./test-entry-points";

/** The entry points that a build in `mode` (vite.config.ts) names. */
async function entryPoints(mode: string): Promise<string[]> {
  const out = await mkdtemp(join(tmpdir(), `cupper-${mode}-`));
  try {
    const outDir = (name: string) => ({ build: { outDir: join(out, name) } });
    const builder = await createBuilder({
      configFile: join(import.meta.dirname, "../vite.config.ts"),
      mode,
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
    return TEST_ENTRY_POINTS.filter((name) => all.includes(name));
  } finally {
    await rm(out, { recursive: true, force: true });
  }
}

it("the test entry points: only e2e builds contain them", { timeout: 120_000 }, async () => {
  expect(await entryPoints("e2e")).toEqual(TEST_ENTRY_POINTS);
  for (const mode of ["release", "self-use", "dev"]) {
    expect(await entryPoints(mode), mode).toEqual([]);
  }
});
