import { describe, expect, test } from "vitest";

import { fold, viewCases } from "../testing";

const cases = viewCases(new URL("../../../../protocol/fixtures/views.json", import.meta.url));

describe("UNIT-views-parity", () => {
  test.each(cases)("$view: $name", (viewCase) => {
    expect(fold(viewCase)).toStrictEqual(viewCase.expected);
  });
});
