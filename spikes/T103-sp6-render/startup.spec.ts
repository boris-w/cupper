// SP-6 scenario 1 (T103; design 5.9.6, 5.9.7): 30 cold and 30 warm starts
// with 200 sessions seeded, one of them of 1,000 items. Cold: the app and
// the host both stopped, the host started by the app; warm: the host left
// running, the app alone ended and started again. The system's file caches
// are not cleared. The very first start of the build, with the system's
// check of a new executable, counts apart.
//
// The product has no `cupper:ready` mark and opens on the new session page,
// never on the last session (a deviation from 5.9.6, in the report): ready
// is the first frame in which the index view is in and the composer takes
// input, timed from just before the launch; the 1,000-item session opened
// by its link to its first rows is timed apart after each start.

import { expect, test, type Cupper } from "../../tools/e2e/src/fixture";
import { untilEvent } from "../../tools/e2e/src/helpers";
import { session } from "../../tools/e2e/src/nfr1";
import { openTimed, record, seed, stats } from "./measure";

// SP6_RUNS: fewer, for a short local smoke run.
const RUNS = Number(process.env.SP6_RUNS ?? 30);

/** Launches the app; from just before it to the first frame ready, and the host's pid. */
async function start(cupper: Cupper) {
  const started = Date.now();
  await cupper.launch();
  const ready = await cupper.window.waitForFunction(
    () => {
      const views = window.__cupperTest.views() as Record<string, { complete?: boolean }>;
      const composer = document.querySelector<HTMLTextAreaElement>("[data-composer] textarea");
      const usable = views.index?.complete && composer && !composer.disabled;
      return usable ? performance.timeOrigin + performance.now() : undefined;
    },
    null,
    { polling: "raf", timeout: 120_000 },
  );
  const ms = ((await ready.jsonValue()) as number) - started;
  const { pid } = await cupper.connected();
  return { ms: Math.round(ms), host: pid as number };
}

const gone = (pid: number) => {
  try {
    process.kill(pid, 0);
    return false;
  } catch {
    return true;
  }
};

/** Ends the app without its quit steps, which would stop the host too (`app.exit`). */
async function endAppOnly(cupper: Cupper) {
  const closed = cupper.app.waitForEvent("close");
  await cupper.app.evaluate(({ app }) => void setImmediate(() => app.exit(0)));
  await closed;
}

test("SP-6 scenario 1: cold and warm starts to ready", async ({ cupper }) => {
  const first = await start(cupper);
  const made = await session(cupper, "say hi");
  await untilEvent(cupper, made, (event) => event.type === "turn.completed");
  const [big] = await seed(cupper, { sessions: 1, turns: 20, items_per_turn: 50 });
  for (let n = 0; n < 4; n++) await seed(cupper, { sessions: 50, turns: 2, items_per_turn: 10 });
  expect(big).toBeDefined();
  const runs: Record<"cold" | "warm", { ready: number[]; open: number[] }> = {
    cold: { ready: [], open: [] },
    warm: { ready: [], open: [] },
  };
  let host = first.host;
  for (const kind of ["cold", "warm"] as const) {
    for (let n = 0; n < RUNS; n++) {
      if (kind === "cold") {
        await cupper.quitWithoutAsking();
        await expect.poll(() => gone(host), { timeout: 30_000 }).toBe(true);
      } else {
        await endAppOnly(cupper);
      }
      const started = await start(cupper);
      if (kind === "warm") expect(started.host, "the same host").toBe(host);
      host = started.host;
      runs[kind].ready.push(started.ms);
      runs[kind].open.push(await openTimed(cupper, big as string));
    }
  }
  record("1-startup", {
    first_start_ms: first.ms,
    cold_ready: stats(runs.cold.ready),
    warm_ready: stats(runs.warm.ready),
    cold_open_1000: stats(runs.cold.open),
    warm_open_1000: stats(runs.warm.open),
    samples: runs,
  });
});
