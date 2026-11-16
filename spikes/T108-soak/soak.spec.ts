// SP-6 scenario 5, the soak (T108; design 5.9.6, 5.9.7): a session on screen
// takes a turn every 5 minutes, a tool call with its output and then 10 s of
// streaming, for about 5 hours. Before the first turn and after each one: the
// working set of the main process and of the main document's and the review
// document's renderers, their JS heaps, the host's RSS, and the sessions and
// events in host.db. The last record sums up how each grew after the warm-up,
// against 5.9.6's limits: recorded only, nothing fails on a number.

import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";

import { test, type Cupper } from "../../tools/e2e/src/fixture";
import { events, untilEvent } from "../../tools/e2e/src/helpers";
import { id, onScreen } from "../../tools/e2e/src/nfr1";
import { record } from "../T103-sp6-render/measure";

const MINUTES = Number(process.env.SOAK_MINUTES ?? 300);
const EVERY_MS = Number(process.env.SOAK_EVERY_SECONDS ?? 300) * 1000;
const WARMUP_MINUTES = Number(process.env.SOAK_WARMUP_MINUTES ?? 30);
/** 5.9.6: the most each may grow after the warm-up, in MB; the heaps have no limit. */
const LIMITS: Record<string, number> = { main_mb: 50, renderer_mb: 100, review_mb: 100, host_mb: 20 };
const MEASURES = [...Object.keys(LIMITS), "main_heap_mb", "renderer_heap_mb", "review_heap_mb"];

type Sample = Record<string, number | undefined>;
const mb = (bytes?: number) => (bytes === undefined ? undefined : Math.round(bytes / 2 ** 20));

/** What each process holds now, in MB, and how many sessions and events there are. */
async function sample(cupper: Cupper, hostPid: number): Promise<Sample> {
  // Electron's working sets are in KB; the renderers are found by their pids.
  const kb = await cupper.app.evaluate(({ app, BrowserWindow }) => {
    const contents = BrowserWindow.getAllWindows()[0]?.webContents;
    const review = contents?.mainFrame.framesInSubtree.find((frame) =>
      frame.url.startsWith("app://cupper-review"),
    );
    const sets = new Map(app.getAppMetrics().map((m) => [m.pid, m.memory.workingSetSize]));
    const of = (pid?: number) => (pid === undefined ? undefined : sets.get(pid));
    const heap = process.memoryUsage().heapUsed / 1024;
    return { main: of(process.pid), renderer: of(contents?.getOSProcessId()), review: of(review?.osProcessId), heap };
  });
  const heap = () => (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory?.usedJSHeapSize;
  const review = cupper.window.frames().find((frame) => frame.url().startsWith("app://cupper-review"));
  const rss = execFileSync("ps", ["-o", "rss=", "-p", String(hostPid)], { encoding: "utf8" });
  const db = new DatabaseSync(join(cupper.dataDir, "host.db"), { readOnly: true });
  const count = (table: string) => (db.prepare(`SELECT count(*) AS n FROM ${table}`).get() as { n: number }).n;
  try {
    const fromKb = (value?: number) => mb(value === undefined ? undefined : value * 1024);
    return {
      main_mb: fromKb(kb.main),
      main_heap_mb: fromKb(kb.heap),
      renderer_mb: fromKb(kb.renderer),
      renderer_heap_mb: mb(await cupper.window.evaluate(heap)),
      review_mb: fromKb(kb.review),
      review_heap_mb: mb(await review?.evaluate(heap)),
      host_mb: fromKb(Number(rss.trim())),
      sessions: count("sessions"),
      events: count("events"),
    };
  } finally {
    db.close();
  }
}

/** The least squares slope of `ys` against `xs` minutes, in MB an hour. */
function slope(xs: number[], ys: number[]): number {
  const mean = (values: number[]) => values.reduce((a, b) => a + b, 0) / values.length;
  const [x, y] = [mean(xs), mean(ys)];
  const cov = xs.reduce((sum, xi, i) => sum + (xi - x) * ((ys[i] ?? y) - y), 0);
  const varx = xs.reduce((sum, xi) => sum + (xi - x) ** 2, 0);
  return varx === 0 ? NaN : Math.round((cov / varx) * 60 * 10) / 10;
}

/** Each measure's growth after the warm-up, first to last sample, its slope, and its limit. */
function growth(samples: (Sample & { minute: number })[]) {
  const after = samples.filter((s) => s.minute >= WARMUP_MINUTES);
  return Object.fromEntries(
    MEASURES.map((key) => {
      const taken = after.filter((s) => s[key] !== undefined);
      const values = taken.map((s) => s[key] as number);
      const grew = values.length ? (values.at(-1) as number) - (values[0] as number) : undefined;
      const limit = LIMITS[key];
      const within = limit === undefined || grew === undefined ? undefined : grew <= limit;
      return [key, { grew_mb: grew, mb_per_hour: slope(taken.map((s) => s.minute), values), limit_mb: limit, within }];
    }),
  );
}

test("SP-6 scenario 5: a turn with a tool call every 5 minutes, for hours", async ({ cupper }) => {
  test.setTimeout((MINUTES + 20) * 60_000);
  const stream = await onScreen(cupper, "say soak");
  const { pid } = await cupper.connected();
  const start = Date.now();
  const minute = () => Math.round((Date.now() - start) / 6000) / 10;
  const samples = [{ minute: 0, ...(await sample(cupper, pid as number)) }];
  record("5-soak", { turn: 0, ...samples[0] });
  for (let n = 1; Date.now() - start < MINUTES * 60_000; n++) {
    const seq = events(cupper, stream).at(-1)?.seq ?? 0;
    const script = `tool t${n} execute run the tests; tool_done t${n} 4000; flood_rate 20 10 80; say done ${n}`;
    await cupper.cli.ok(["send", id(stream), script]);
    await untilEvent(cupper, stream, (e) => e.type === "turn.completed" && e.seq > seq);
    const taken = { minute: minute(), ...(await sample(cupper, pid as number)) };
    samples.push(taken);
    record("5-soak", { turn: n, ...taken });
    await cupper.window.waitForTimeout(Math.max(0, start + n * EVERY_MS - Date.now()));
  }
  record("5-soak", { soaked_minutes: minute(), warmup_minutes: WARMUP_MINUTES, growth: growth(samples) });
});
