import { describe, expect, test } from "vitest";

import { idsIn, isId, owner } from "./ids.ts";

describe("UNIT-trace-ids: IDs", () => {
  test("are found in text by their exact spelling", () => {
    const text =
      "新增 UNIT-ci-summary、UNIT-ci-lock；IT-NFR9-failpoints-core (`GOLD-A-option-map`), " +
      "MAN-S1. PRD UI-0. XUNIT-x, my_UNIT-y and UNIT- are not IDs; IT-K1- ends at K1.";
    expect(idsIn(text)).toEqual([
      "UNIT-ci-summary",
      "UNIT-ci-lock",
      "IT-NFR9-failpoints-core",
      "GOLD-A-option-map",
      "MAN-S1",
      "UI-0",
      "IT-K1",
    ]);
    expect(["UNIT-ci-lock", "GOLD-B-codex-s07", "SMOKE-SIGNED-reattach"].every(isId)).toBe(true);
    expect(
      ["UNIT", "UNIT-", "UNIT-a--b", "UNIT-a-", "unit-a", "UNITS-a", "UNIT-ä"].some(isId),
    ).toBe(false);
  });

  test("own a test whose name or path segment starts with them", () => {
    const ids = [
      "UNIT-ci-lock",
      "UNIT-ci-summary",
      "IT-NFR9-failpoints",
      "IT-NFR9-failpoints-core",
    ];
    // Rust: snake case, followed by `_` or the end of the segment; the longest ID wins.
    expect(owner(ids, "ci::lock::tests::unit_ci_lock_refuses_dirty_tree")).toBe("UNIT-ci-lock");
    expect(owner(ids, "it_nfr9_failpoints_core::kill")).toBe("IT-NFR9-failpoints-core");
    expect(owner(ids, "it_nfr9_failpoints_git_x")).toBe("IT-NFR9-failpoints");
    expect(owner(ids, "it_nfr9_failpointsx")).toBeUndefined();
    expect(owner(ids, "ci::tests::unit_ci_locked_out")).toBeUndefined();
    // JS and Playwright: verbatim, followed by anything that cannot continue an ID.
    expect(owner(ids, "a.test.ts::UNIT-ci-lock::waits")).toBe("UNIT-ci-lock");
    expect(owner(ids, "a.test.ts::UNIT-ci-summary: no paths")).toBe("UNIT-ci-summary");
    expect(owner(ids, "e2e/a.spec.ts::IT-NFR9-failpoints @smoke")).toBe("IT-NFR9-failpoints");
    for (const name of [
      "a.test.ts::UNIT-ci-lock-x",
      "a.test.ts::unit-ci-lock",
      "waits UNIT-ci-lock",
    ]) {
      expect(owner(ids, name), name).toBeUndefined();
    }
  });
});
