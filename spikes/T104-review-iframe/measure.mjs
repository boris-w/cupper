// Scenario 9 on the runner: N cold runs of the spike, each with a fresh
// userData, into results/run-<n>.json, then results/summary.json: p50, p95
// and max of each number, every check that differs between runs. Recorded
// only. On the runner the first run also takes the screenshots, writes the
// clipboard and sends a real keystroke; never on someone's own Mac.
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const runs = Number(process.argv[2] ?? 30);
const electron = "../../apps/desktop/node_modules/.bin/electron";
mkdirSync("results", { recursive: true });
const results = [];
for (let n = 1; n <= runs; n++) {
  const userData = mkdtempSync(join(tmpdir(), "t104-"));
  const first = n === 1 && process.env.GITHUB_ACTIONS === "true" ? "1" : "";
  const env = { ...process.env, T104_RESULTS: `results/run-${n}.json`, T104_USER_DATA: userData };
  Object.assign(env, { T104_SHOTS: first, T104_CLIPBOARD: first, T104_KEYSTROKE: first });
  spawnSync(electron, ["dist/main.cjs"], { env, stdio: "inherit", timeout: 180_000 });
  rmSync(userData, { recursive: true, force: true });
  results.push(JSON.parse(readFileSync(`results/run-${n}.json`, "utf8")));
}
const ok = results.filter((r) => !r.error);
const stats = (values) => {
  const sorted = values.toSorted((a, b) => a - b);
  const at = (q) => sorted[Math.min(sorted.length - 1, Math.ceil(q * sorted.length) - 1)];
  return { n: sorted.length, p50: at(0.5), p95: at(0.95), max: sorted.at(-1) };
};
const mb = (kb) => Math.round(kb / 1024);
const summary = {
  runs,
  failed: results.filter((r) => r.error).map((r) => r.error),
  coldStartMs: Object.fromEntries(
    ["readyMs", "rowsMs", "highlightedMs", "sinceLaunchMs"].map((k) => [k, stats(ok.map((r) => r.timings[k]))]),
  ),
  memoryMb: {
    reviewRenderer: stats(ok.map((r) => mb(r.memory.reviewRenderer.workingSetSize))),
    mainRenderer: stats(ok.map((r) => mb(r.memory.mainRenderer.workingSetSize))),
    allProcesses: stats(ok.map((r) => mb(r.memory.all.reduce((n, p) => n + p.memory.workingSetSize, 0)))),
  },
  scrollFrameMs: stats(ok.flatMap((r) => r.scrollFrames)),
  violations: ok.reduce((n, r) => n + r.violations.main + r.violations.review + r.violations.console, 0),
  checks: ok[0]?.checks,
  differing: Object.keys(ok[0]?.checks ?? {}).filter(
    (k) => !["keystroke", "clipboardWrite", "seenByMain"].includes(k) && new Set(ok.map((r) => JSON.stringify(r.checks[k]))).size > 1,
  ),
};
writeFileSync("results/summary.json", JSON.stringify(summary, null, 1));
console.log(JSON.stringify(summary.coldStartMs), JSON.stringify(summary.scrollFrameMs), summary.violations);
