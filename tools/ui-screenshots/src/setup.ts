// Playwright's global setup: builds the screenshot page, then checks what the
// baselines were made on, or records it for `pnpm ui:baseline`, which the
// `ui-baseline` workflow runs on the CI runner (T197).

import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { chromium } from "@playwright/test";
import { build } from "vite";

import { compareMeta, type BaselineMeta } from "./meta";

const META = new URL("../baselines/meta.json", import.meta.url);

export default async function setup() {
  await build({ configFile: fileURLToPath(new URL("../vite.config.ts", import.meta.url)) });
  const browser = await chromium.launch();
  const current: BaselineMeta = {
    chromium: browser.version(),
    macos: execFileSync("sw_vers", ["-productVersion"], { encoding: "utf8" }).trim(),
  };
  await browser.close();
  if (process.env.CUPPER_UI_BASELINE) {
    await writeFile(META, `${JSON.stringify(current, null, 2)}\n`);
    return;
  }
  const baseline = JSON.parse(await readFile(META, "utf8")) as BaselineMeta;
  const { ok, notes } = compareMeta(baseline, current);
  notes.forEach((note) => console.warn(`ui: ${note}`));
  if (!ok) {
    throw new Error("the baselines were made on another Chromium or macOS (see above)");
  }
}
