import { describe, expect, test } from "vitest";

import { fold, viewCases } from "../testing";

// Written by the Rust half (`view::parity`) in `cargo xtask ci differential`, nightly.
const file = process.env["CUPPER_VIEW_CASES"];

describe.skipIf(file === undefined)("UNIT-views-differential", () => {
  test("random host streams fold into the views Rust folds them into", () => {
    const cases = viewCases(file ?? "");
    expect(cases.length).toBeGreaterThan(0);
    for (const viewCase of cases) {
      expect(fold(viewCase), viewCase.name).toStrictEqual(viewCase.expected);
    }
  });
});
