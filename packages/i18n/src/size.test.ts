import { expect, test } from "vitest";

import { formatBytes } from "./size";

test("formatBytes: one decimal below 10 of a unit, whole numbers above", () => {
  const sizes = [0, 512, 2048, 4.5 * 2 ** 20, 120 * 2 ** 30, 3 * 2 ** 50];
  expect(sizes.map(formatBytes)).toEqual(["0 B", "512 B", "2.0 KB", "4.5 MB", "120 GB", "3072 TB"]);
});
