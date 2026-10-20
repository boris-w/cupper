import { describe, expect, test } from "vitest";

import { compareMeta } from "./meta";

const baseline = { chromium: "156.0.8078.4", macos: "26.4" };

describe("UNIT-ui-baseline-meta", () => {
  test("the same Chromium and macOS pass without a note", () => {
    expect(compareMeta(baseline, { ...baseline })).toEqual({ ok: true, notes: [] });
  });

  test("another Chromium fails", () => {
    const check = compareMeta(baseline, { ...baseline, chromium: "156.0.8078.5" });
    expect(check.ok).toBe(false);
    expect(check.notes).toEqual([expect.stringContaining("Chromium 156.0.8078.5")]);
  });

  test("a minor macOS update passes with a note", () => {
    const check = compareMeta(baseline, { ...baseline, macos: "26.4.1" });
    expect(check.ok).toBe(true);
    expect(check.notes).toEqual([expect.stringContaining("pnpm ui:baseline")]);
  });

  test("another major macOS fails", () => {
    expect(compareMeta(baseline, { ...baseline, macos: "27.0" }).ok).toBe(false);
  });
});
