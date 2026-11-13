import { describe, expect, test } from "vitest";

import { fold, viewCases } from "../testing";

// Written by the Rust half (`view::parity`) in `cargo xtask ci differential`, nightly.
const file = process.env["CUPPER_VIEW_CASES"];
const cases = file === undefined ? [] : viewCases(file);

describe.skipIf(file === undefined)("UNIT-views-differential", () => {
  test("the Rust half wrote random host streams", () => {
    expect(cases.length).toBeGreaterThan(0);
  });

  // A test per stream: the 900 of them take 3.4 s on a runner, and a slow
  // runner took 20 s, past one test's time limit (T260); each stream takes
  // milliseconds.
  test.each(cases)("$name folds into the view Rust folds it into", (viewCase) => {
    expect(fold(viewCase)).toStrictEqual(viewCase.expected);
  });
});
