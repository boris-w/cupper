// What SP-6's scenarios share (T103): numbers summed up and written to
// `results/`, the page's animation frames, the processes' CPU and memory,
// sessions seeded through `test/seed`, and a session opened by its link,
// timed to its first rows on screen. Times are milliseconds; the renderer's
// are on the wall clock (`performance.timeOrigin + now`), as are the main
// process's and the host's.

import { execFileSync } from "node:child_process";
import { appendFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";

import type { Cupper } from "../../tools/e2e/src/fixture";
import { harness } from "../../tools/e2e/src/perf";

export const RESULTS = process.env.SP6_RESULTS ?? join(import.meta.dirname, "results");

const round = (value: number) => Math.round(value * 10) / 10;

/** n, p50, p95 and the max, by nearest rank; with few samples p95 is the max. */
export function stats(values: number[]) {
  const sorted = [...values].sort((a, b) => a - b);
  const at = (q: number) => sorted[Math.max(0, Math.ceil(q * sorted.length) - 1)] ?? NaN;
  return { n: sorted.length, p50: round(at(0.5)), p95: round(at(0.95)), max: round(at(1)) };
}

/** One line of `results/<scenario>.jsonl`, also on the console for the run's log. */
export function record(scenario: string, data: Record<string, unknown>) {
  mkdirSync(RESULTS, { recursive: true });
  const line = JSON.stringify({ scenario, ...data });
  appendFileSync(join(RESULTS, `${scenario}.jsonl`), `${line}\n`);
  console.log(`sp6 ${line}`);
}

interface Frames {
  intervals: number[];
  on: boolean;
}
type Sp6Window = Window & { __sp6Frames?: Frames; __sp6Screen?: number };

/** Starts taking the intervals between the page's animation frames. */
export const startFrames = (cupper: Cupper) =>
  cupper.window.evaluate(() => {
    const frames: Frames = { intervals: [], on: true };
    let last = 0;
    const tick = (now: number) => {
      if (last) frames.intervals.push(now - last);
      last = now;
      if (frames.on) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    (window as Sp6Window).__sp6Frames = frames;
  });

/** The frame intervals since `startFrames`, summed up; taking stops. */
export async function stopFrames(cupper: Cupper) {
  const intervals = await cupper.window.evaluate(() => {
    const frames = (window as Sp6Window).__sp6Frames;
    if (frames) frames.on = false;
    return frames?.intervals ?? [];
  });
  return stats(intervals);
}

/**
 * Samples the app's processes and the host's once a second until `stop`:
 * the share of a core each type of process used (Electron's `percentCPUUsage`,
 * `ps` for the host) and the most memory each held, in MB.
 */
export function sampleUsage(cupper: Cupper, hostPid: number) {
  const cpu = new Map<string, number[]>();
  const memory = new Map<string, number>();
  const add = (kind: string, share: number, mb: number) => {
    cpu.set(kind, [...(cpu.get(kind) ?? []), share]);
    memory.set(kind, Math.max(memory.get(kind) ?? 0, round(mb)));
  };
  const sample = async () => {
    const metrics = await cupper.app.evaluate(({ app }) =>
      app.getAppMetrics().map((m) => ({ type: m.type, cpu: m.cpu.percentCPUUsage, kb: m.memory.workingSetSize })),
    );
    for (const m of metrics) add(m.type, m.cpu, m.kb / 1024);
    const ps = execFileSync("ps", ["-o", "%cpu=,rss=", "-p", String(hostPid)], { encoding: "utf8" });
    const [share = NaN, kb = NaN] = ps.trim().split(/\s+/).map(Number);
    add("host", share, kb / 1024);
  };
  const timer = setInterval(() => void sample().catch(() => undefined), 1000);
  return () => {
    clearInterval(timer);
    const kinds = [...cpu.keys()];
    return Object.fromEntries(kinds.map((k) => [k, { cpu: stats(cpu.get(k) ?? []), mb: memory.get(k) }]));
  };
}

/**
 * The host's `test/seed` (design 5.9.9) with the SP-1 mix; the sessions it
 * made, by id. A large seed takes longer than a request's usual 10 s.
 */
export async function seed(
  cupper: Cupper,
  profile: { sessions: number; turns: number; items_per_turn: number; large_turn_items?: number },
) {
  const before = new Set(await sessionIds(cupper));
  const { peer } = await harness(cupper);
  peer.send({ jsonrpc: "2.0", id: 1, method: "test/seed", params: { profile: { ...profile, mix: "sp1" } } });
  const reply = await peer.next((m) => m.id === 1 && !("method" in m), 15 * 60_000);
  peer.socket.end();
  if (reply.error) throw new Error(`test/seed: ${JSON.stringify(reply.error)}`);
  return (await sessionIds(cupper)).filter((id) => !before.has(id));
}

/** The events of `stream` in `host.db`: seq, type, the host's wall clock time and data. */
export function stored(cupper: Cupper, stream: string) {
  const db = new DatabaseSync(join(cupper.dataDir, "host.db"), { readOnly: true });
  try {
    const rows = db
      .prepare("SELECT json FROM events WHERE stream = ? ORDER BY seq")
      .all(stream) as { json: string }[];
    return rows.map(
      ({ json }) =>
        JSON.parse(json) as { seq: number; type: string; ts: number; data: Record<string, unknown> },
    );
  } finally {
    db.close();
  }
}

/** Every session's id, as the CLI lists them. */
export async function sessionIds(cupper: Cupper): Promise<string[]> {
  const { lines } = await cupper.cli.run(["sessions"]);
  return lines.map((line) => String(line.session_id));
}

/**
 * Opens session `id` through its link (design 5.7.2), as a click on a
 * notification does, and times it: from the link in the main process to
 * the first frame with rows that were not on screen before.
 */
export async function openTimed(cupper: Cupper, id: string): Promise<number> {
  await cupper.window.evaluate(() => {
    const rows = () => document.querySelectorAll<HTMLElement>("[data-stream-list] [data-index]");
    rows().forEach((row) => (row.dataset.sp6Old = ""));
    const page = window as Sp6Window;
    page.__sp6Screen = undefined;
    const look = () => {
      if ([...rows()].some((row) => row.dataset.sp6Old === undefined)) {
        page.__sp6Screen = performance.timeOrigin + performance.now();
      } else requestAnimationFrame(look);
    };
    requestAnimationFrame(look);
  });
  const sent = await cupper.app.evaluate(({ app }, url) => {
    const at = Date.now();
    app.emit("open-url", { preventDefault: () => undefined }, url);
    return at;
  }, `cupper-e2e://session/${id}`);
  const shown = await cupper.window.waitForFunction(() => (window as Sp6Window).__sp6Screen, null, {
    timeout: 120_000,
  });
  return round(((await shown.jsonValue()) as number) - sent);
}
