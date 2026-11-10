const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/** A size in bytes as people read it, 1024 to a step: `512 B`, `3.4 MB`, `120 GB`. */
export function formatBytes(bytes: number): string {
  let n = bytes;
  let unit = 0;
  while (n >= 1024 && unit < UNITS.length - 1) {
    n /= 1024;
    unit += 1;
  }
  const shown = unit > 0 && n < 10 ? n.toFixed(1) : Math.round(n).toString();
  return `${shown} ${UNITS[unit]}`;
}
