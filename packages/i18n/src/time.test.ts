import { describe, expect, test } from "vitest";

import { formatDivider, formatRelative, formatStamp } from "./time";

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

  test("in the locale's words, a space between the number and its unit", () => {
    expect(formatRelative(now - 2 * 86400 * 1000, now, "zh-CN")).toBe("前天");
    expect(formatRelative(now - 5 * 60 * 1000, now, "zh-CN")).toBe("5 分钟前");
    expect(formatRelative(now - 65 * 86400 * 1000, now, "zh-CN")).toBe("2 个月前");
  });
});

describe("formatDivider and formatStamp: when a turn started, and ended", () => {
  const now = new Date(2026, 10, 23, 15, 0).getTime();
  const at = (days: number) => new Date(2026, 10, 23 - days, 9, 5).getTime();
  test("the day: today and yesterday, the weekday within a week, then the date, its year after a year", () => {
    const days = [0, 1, 3, 30, 400].map((d) => formatDivider(at(d), now, "zh-CN").day);
    expect(days).toEqual(["今天", "昨天", "星期五", "10月24日周六", "2025年10月19日"]);
    expect(formatDivider(at(0), now, "zh-CN").time).toBe("9:05");
  });
  test("under an answer: the time, after the date on another day", () => {
    expect([0, 1, 400].map((d) => formatStamp(at(d), now, "zh-CN"))).toEqual([
      ...["9:05", "11月22日 9:05", "2025年10月19日 9:05"],
    ]);
  });
});
