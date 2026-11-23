// When something happened, as people say it (design 5.7.4, T106): "5 分钟前",
// "昨天", "2 个月前", in the largest unit the time reaches.

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["second", 60],
  ["minute", 60],
  ["hour", 24],
  ["day", 30],
  ["month", 12],
];

/**
 * `at` relative to `now`, both in ms, in the words of `locale`; a space
 * between the number and its unit, as Codex writes "5 分钟前" (T106 Q38).
 */
export function formatRelative(at: number, now: number, locale: string): string {
  const format = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  const spaced = (value: number, unit: Intl.RelativeTimeFormatUnit) =>
    format.format(Math.trunc(value), unit).replace(/(\d)(?=\p{L})/gu, "$1 ");
  let value = (at - now) / 1000;
  for (const [unit, size] of UNITS) {
    if (Math.abs(value) < size) {
      return spaced(value, unit);
    }
    value /= size;
  }
  return spaced(value, "year");
}

/** The calendar day of `ms`, local, as a day count. */
const dayOf = (ms: number) => {
  const date = new Date(ms);
  return Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / 86_400_000;
};

/**
 * The day and the time of `at` above a turn, as Codex's divider says them:
 * "今天", "昨天", the weekday within a week, then the date, its year after a
 * year; the time of day.
 */
export function formatDivider(at: number, now: number, locale: string) {
  const days = Math.max(dayOf(now) - dayOf(at), 0);
  const format = (options: Intl.DateTimeFormatOptions) =>
    new Intl.DateTimeFormat(locale, options).format(at);
  const day =
    days <= 1
      ? new Intl.RelativeTimeFormat(locale, { numeric: "auto" }).format(-days, "day")
      : days <= 7
        ? format({ weekday: "long" })
        : format({
            ...(days > 365 ? { year: "numeric" } : { weekday: "short" }),
            ...{ month: "short", day: "numeric" },
          });
  return { day, time: format({ hour: "numeric", minute: "2-digit" }) };
}

/** When a turn ended, under its answer: the time, after the date on another day. */
export function formatStamp(at: number, now: number, locale: string): string {
  const other = dayOf(at) !== dayOf(now);
  const year = new Date(at).getFullYear() !== new Date(now).getFullYear();
  return new Intl.DateTimeFormat(locale, {
    ...(year && { year: "numeric" }),
    ...(other && { month: "short", day: "numeric" }),
    hour: "numeric",
    minute: "2-digit",
  }).format(at);
}
