import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

import { expect, test } from "vitest";

const at = (path: string) => resolve(import.meta.dirname, path);
const LOCALES = at("../../../packages/i18n/src/locales");

// The screenshots fall back to a system font for any character the fixture
// fonts lack. Text outside the subset needs a new subset (see the T118 notes).
test("the Chinese fixture font has every character of the UI text", () => {
  const subset = new Set(readFileSync(at("../fonts/noto-sans-sc-chars.txt"), "utf8"));
  const missing = new Set<string>();
  for (const locale of readdirSync(LOCALES)) {
    for (const file of readdirSync(resolve(LOCALES, locale))) {
      const text = readFileSync(resolve(LOCALES, locale, file), "utf8");
      [...text].filter((char) => char > "~" && !subset.has(char)).forEach((c) => missing.add(c));
    }
  }
  expect([...missing].join("")).toBe("");
});
