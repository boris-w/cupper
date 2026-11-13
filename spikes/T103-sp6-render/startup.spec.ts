// SP-6 scenario 1 (T103; design 5.9.6, 5.9.7): 30 cold starts with 200
// sessions seeded, one of them of 1,000 items: the app and the host both
// stopped, the host started by the app. The system's file caches are not
// cleared. The very first start of the build, with the system's check of a
// new executable, counts apart. Warm starts cannot be had on an e2e build:
// its host takes `dev_key` only from the process that started it, and once
// that app is gone it ends (design 5.8.1); the first warm start waited out
// its 120 s (spike-run 43410584669).
//
// Ready is the renderer's `cupper:ready` (T262): the index applied and the
// composer on screen. The app opens on the new session page, never on the
// last session (Q45), so the 1,000-item session opened by its link to its
// first rows is timed apart after each start. Each start is split, from
// just before the launch: the main process, the host and the control
// connection (their logs), then the page's navigation start and DOM ready,
// the window's connection, the index and ready (the renderer's marks).

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Cupper } from "../../tools/e2e/src/fixture";
import { untilEvent } from "../../tools/e2e/src/helpers";
import { session } from "../../tools/e2e/src/nfr1";
import { openTimed, record, seed, stats } from "./measure";

// SP6_RUNS: fewer, for a short local smoke run.
const RUNS = Number(process.env.SP6_RUNS ?? 30);

/**
 * Launches the app; from just before it to the renderer's `cupper:ready`
 * (T262), the host's pid, and the renderer's own stretch: the page's
 * navigation start and its DOM ready, then its marks, all on the wall clock.
 */
async function start(cupper: Cupper) {
  const started = Date.now();
  await cupper.launch();
  await cupper.window.waitForFunction(() => performance.getEntriesByName("cupper:ready").length > 0, null, {
    timeout: 120_000,
  });
  const page = await cupper.window.evaluate(() => {
    const at = (ms: number | undefined) => (ms === undefined ? null : performance.timeOrigin + ms);
    const mark = (name: string) => at(performance.getEntriesByName(name)[0]?.startTime);
    const navigation = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
    return {
      page: performance.timeOrigin,
      dom: at(navigation?.domContentLoadedEventEnd),
      window: mark("cupper:connected"),
      index: mark("cupper:index"),
      ready: mark("cupper:ready"),
    };
  });
  const since = (at: number | null) => (at === null ? null : Math.round(at - started));
  const { pid } = await cupper.connected();
  const renderer = Object.fromEntries(Object.entries(page).map(([k, v]) => [k, since(v)]));
  return { ms: since(page.ready) ?? NaN, host: pid as number, phases: { ...phases(cupper, started), ...renderer } };
}

/**
 * Where a start spent its time, from the logs, in ms after `since`: the main
 * process up, the host up (its first log line), the control connection ready.
 */
function phases(cupper: Cupper, since: number) {
  const first = (file: string, test: (entry: Record<string, unknown>) => boolean) => {
    const lines = readFileSync(file, "utf8").split("\n").filter(Boolean);
    const entries = lines.map((line) => JSON.parse(line) as Record<string, unknown>);
    const at = entries
      .map((entry) => ({ entry, at: Date.parse(String(entry.time)) }))
      .find(({ entry, at }) => at >= since && test(entry))?.at;
    return at === undefined ? null : at - since;
  };
  const main = join(cupper.root, "logs", "main.log");
  return {
    main: first(main, (e) => e.msg === "started"),
    host: first(join(cupper.dataDir, "logs", "host.log"), (e) => e.message === "the host started"),
    control: first(main, (e) => e.msg === "host connection" && e.status === "ready"),
  };
}

const gone = (pid: number) => {
  try {
    process.kill(pid, 0);
    return false;
  } catch {
    return true;
  }
};

test("SP-6 scenario 1: cold starts to ready", async ({ cupper }) => {
  const first = await start(cupper);
  const made = await session(cupper, "say hi");
  await untilEvent(cupper, made, (event) => event.type === "turn.completed");
  const [big] = await seed(cupper, { sessions: 1, turns: 20, items_per_turn: 50 });
  for (let n = 0; n < 4; n++) await seed(cupper, { sessions: 50, turns: 2, items_per_turn: 10 });
  expect(big).toBeDefined();
  const runs: { ready: number[]; open: number[]; phases: unknown[] } = {
    ready: [],
    open: [],
    phases: [],
  };
  let host = first.host;
  for (let n = 0; n < RUNS; n++) {
    await cupper.quitWithoutAsking();
    await expect.poll(() => gone(host), { timeout: 30_000 }).toBe(true);
    const started = await start(cupper);
    host = started.host;
    runs.ready.push(started.ms);
    runs.phases.push(started.phases);
    runs.open.push(await openTimed(cupper, big as string));
  }
  record("1-startup", {
    first_start_ms: first.ms,
    cold_ready: stats(runs.ready),
    cold_open_1000: stats(runs.open),
    samples: runs,
  });
});
