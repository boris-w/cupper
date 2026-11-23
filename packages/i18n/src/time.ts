// When something happened, as people say it (design 5.7.4, T106): "5 分钟前",
// "昨天", "2 个月前", in the largest unit the time reaches.

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["second", 60],
  ["minute", 60],
  ["hour", 24],
  ["day", 30],
  ["month", 12],
];

/** `at` relative to `now`, both in ms, in the words of `locale`. */
export function formatRelative(at: number, now: number, locale: string): string {
  const format = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  let value = (at - now) / 1000;
  for (const [unit, size] of UNITS) {
    if (Math.abs(value) < size) {
      return format.format(Math.trunc(value), unit);
    }
    value /= size;
  }
  return format.format(Math.trunc(value), "year");
}
