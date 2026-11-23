import { describe, expect, test } from "vitest";

import { formatRelative } from "./time";

describe("formatRelative", () => {
  const now = Date.UTC(2026, 10, 23, 12);
  const ago = (seconds: number) => formatRelative(now - seconds * 1000, now, "en");

  test("takes the largest unit the time reaches", () => {
    expect(ago(0)).toBe("now");
    expect(ago(59)).toBe("59 seconds ago");
    expect(ago(5 * 60)).toBe("5 minutes ago");
    expect(ago(3 * 3600 + 59 * 60)).toBe("3 hours ago");
    expect(ago(86400)).toBe("yesterday");
    expect(ago(65 * 86400)).toBe("2 months ago");
    expect(ago(800 * 86400)).toBe("2 years ago");
  });

  test("in the locale's words", () => {
    expect(formatRelative(now - 2 * 86400 * 1000, now, "zh-CN")).toBe("前天");
  });
});
