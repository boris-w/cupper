/**
 * Scenario 9's diff: a synthetic TypeScript file of 5,000 lines against the
 * same with every fifth line changed, one hunk of about 6,000 rows. One line
 * carries markup, which must show as text (security (i)).
 */
export function fixture(lines = 5000) {
  const line = (n: number, v: string) => `export const value${n} = compute(${n}, "${v}"); // ${n}`;
  const old = Array.from({ length: lines }, (_, n) => line(n, "a"));
  const changed = old.map((text, n) => (n % 5 === 0 ? line(n, "b") : text));
  changed[2] = `const html = "<style>body{display:none}</style><img src=x onerror=alert(1)>";`;
  return { old: old.join("\n") + "\n", changed: changed.join("\n") + "\n" };
}
