// The measuring helpers of the PERF cases (T078, T220; SP-6, T103, T155):
// the test's own `test_harness` connection to the host, a blob uploaded on
// it, the app's window covered, the page's animation frames, the processes'
// CPU and memory, seeded sessions, sessions timed to their first rows, and
// the samples handed to `CUPPER_PERF_OUT`. Times are milliseconds on the
// wall clock: the renderer's are `performance.timeOrigin + now` (T262).

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { appendFileSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";

import { expect, type Cupper } from "./fixture";
import { dial, initialize, keyProof, type Peer } from "./raw";

/** The test's own connection to the host, as `test_harness`, and a way to call on it. */
export async function harness(cupper: Cupper) {
  const peer = await dial(cupper.dataDir);
  const key = Buffer.from(readFileSync(join(cupper.root, "harness_key"), "utf8").trim(), "hex");
  const init = await initialize(peer, keyProof("harness", key, peer.nonce));
  expect(init.error).toBeUndefined();
  let next = 10;
  const call = async (method: string, params: unknown) => {
    const { result, error } = await peer.request(next++, method, params);
    expect(error, method).toBeUndefined();
    return result;
  };
  return { peer, call };
}

/** Uploads `bytes` as a blob (design 5.3.1): its chunks, then `blob/put`; its hash. */
export async function putBlob(
  peer: Peer,
  call: (m: string, p: unknown) => Promise<unknown>,
  bytes: Buffer,
) {
  const hash = createHash("sha256").update(bytes).digest("hex");
  const most = 64_000 - 5 - 41;
  for (let offset = 0; offset < bytes.length; offset += most) {
    const head = Buffer.alloc(41);
    Buffer.from(hash, "hex").copy(head);
    head.writeBigUInt64BE(BigInt(offset), 32);
    head[40] = offset + most >= bytes.length ? 1 : 0;
    peer.send(Buffer.concat([head, bytes.subarray(offset, offset + most)]), 0x02);
  }
  await call("blob/put", { hash, size: bytes.length, media_type: "application/octet-stream" });
  return hash;
}

/**
 * Covers the app's window with another one, and stops its animation frames.
 * Under Playwright, Chromium keeps the page visible and its frames running
 * even covered, minimized or hidden (T078), so the frames are stopped by
 * hand: what a hidden page loses that the store cares about (design 5.7.1).
 * SP-6's scenario 8 keeps the frames, to see what Chromium itself does.
 */
export async function cover(cupper: Cupper, { stopFrames = true } = {}) {
  const bounds = await (
    await cupper.app.browserWindow(cupper.window)
  ).evaluate((w) => w.getBounds());
  await cupper.app.evaluate(({ BrowserWindow }, bounds) => {
    // Not shown on creation, which would activate the app (T195).
    const cover = new BrowserWindow({
      ...bounds,
      show: false,
      frame: false,
      alwaysOnTop: true,
      focusable: false,
    });
    cover.showInactive();
  }, bounds);
  if (!stopFrames) return;
  await cupper.window.evaluate(() => {
    window.requestAnimationFrame = () => 0;
  });
}

/** The page's animation frames in one second. */
export const frameRate = (cupper: Cupper) =>
  cupper.window.evaluate(
    () =>
      new Promise<number>((done) => {
        let frames = 0;
        const tick = () => void (frames++, requestAnimationFrame(tick));
        requestAnimationFrame(tick);
        setTimeout(() => done(frames), 1000);
      }),
  );

/** n, p50, p95 and the max, by nearest rank; with few samples p95 is the max. */
export function stats(values: number[]) {
  const sorted = [...values].sort((a, b) => a - b);
  const at = (q: number) => sorted[Math.max(0, Math.ceil(q * sorted.length) - 1)] ?? NaN;
  const round = (value: number) => Math.round(value * 10) / 10;
  return { n: sorted.length, p50: round(at(0.5)), p95: round(at(0.95)), max: round(at(1)) };
}

/**
 * A PERF metric's samples: to `CUPPER_PERF_OUT`, where the nightly works out
 * n, p50, p95 and the max for its summary (`xtask` `perf::trend`), and to the
 * log. Recorded only (M1a sync D1).
 */
export function perfSamples(metric: string, unit: string, samples: number[]) {
  const out = process.env.CUPPER_PERF_OUT;
  if (out && samples.length > 0) {
    appendFileSync(out, `${JSON.stringify({ metric, unit, samples })}\n`);
  }
  console.log(`perf ${metric} ${JSON.stringify(stats(samples))} ${unit}`);
}

type Sampled = Window & { __perfFrames?: { intervals: number[]; on: boolean } };

/** Starts taking the intervals between the page's animation frames. */
export const startFrames = (cupper: Cupper) =>
  cupper.window.evaluate(() => {
    const frames = { intervals: [] as number[], on: true };
    let last = 0;
    const tick = (now: number) => {
      if (last) frames.intervals.push(now - last);
      last = now;
      if (frames.on) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    (window as Sampled).__perfFrames = frames;
  });

/** The frame intervals since `startFrames`, in ms; taking stops. */
export const stopFrames = (cupper: Cupper) =>
  cupper.window.evaluate(() => {
    const frames = (window as Sampled).__perfFrames;
    if (frames) frames.on = false;
    return frames?.intervals ?? [];
  });

/** A process's CPU time so far in seconds, from `ps` (`[[dd-]hh:]mm:ss.cc`). */
function cpuSeconds(pid: number): number {
  const time = execFileSync("ps", ["-o", "time=", "-p", String(pid)], { encoding: "utf8" }).trim();
  const [days, clock = days] = time.includes("-") ? time.split("-") : ["0", time];
  const parts = (clock ?? "0").split(":").map(Number);
  return Number(days) * 86_400 + parts.reduce((total, part) => total * 60 + part, 0);
}

/**
 * Samples the CPU and memory of the app's processes, by type, and the
 * host's, once a second until the returned stop. CPU is in percent of one
 * core over each second: Electron's `percentCPUUsage` is the share of all
 * the machine's cores since the last `getAppMetrics` (Electron 45 divides by
 * `base::SysInfo::NumberOfProcessors`), so it is multiplied back; the host's
 * comes from its CPU time in `ps`. Memory is the most each held, in MB.
 */
export async function sampleUsage(cupper: Cupper, hostPid: number) {
  const cores = await cupper.app.evaluate(() => process.getBuiltinModule("node:os").cpus().length);
  const cpu = new Map<string, number[]>();
  const memory = new Map<string, number>();
  const add = (kind: string, share: number, mb: number) => {
    cpu.set(kind, [...(cpu.get(kind) ?? []), Math.round(share * 10) / 10]);
    memory.set(kind, Math.max(memory.get(kind) ?? 0, Math.round(mb)));
  };
  let host = { at: Date.now(), cpu: cpuSeconds(hostPid) };
  const sample = async () => {
    const metrics = await cupper.app.evaluate(({ app }) =>
      app
        .getAppMetrics()
        .map((m) => ({ type: m.type, cpu: m.cpu.percentCPUUsage, kb: m.memory.workingSetSize })),
    );
    for (const m of metrics) add(m.type, m.cpu * cores, m.kb / 1024);
    const now = { at: Date.now(), cpu: cpuSeconds(hostPid) };
    const rss = Number(
      execFileSync("ps", ["-o", "rss=", "-p", String(hostPid)], { encoding: "utf8" }),
    );
    add("host", ((now.cpu - host.cpu) * 100_000) / (now.at - host.at), rss / 1024);
    host = now;
  };
  await sample();
  cpu.clear();
  const timer = setInterval(() => void sample().catch(() => undefined), 1000);
  return () => {
    clearInterval(timer);
    return { cpu, memory };
  };
}

/**
 * `test/seed` (design 5.9.9) on the test's own connection; the sessions it
 * made, by id. A large seed takes longer than a request's 10 s, and the
 * connection keeps `initialize`'s response, id 1, among its messages.
 */
export async function seed(
  cupper: Cupper,
  profile: {
    sessions: number;
    turns: number;
    items_per_turn: number;
    mix: "sp1" | "sp1_code";
    large_turn_items?: number;
  },
) {
  const before = new Set(await sessionIds(cupper));
  const { peer } = await harness(cupper);
  peer.send({ jsonrpc: "2.0", id: 99, method: "test/seed", params: { profile } });
  const reply = await peer.next((m) => m.id === 99 && !("method" in m), 15 * 60_000);
  peer.socket.end();
  expect(reply.error, "test/seed").toBeUndefined();
  return (await sessionIds(cupper)).filter((id) => !before.has(id));
}

/** Every session's id, as the CLI lists them. */
export async function sessionIds(cupper: Cupper): Promise<string[]> {
  const { lines } = await cupper.cli.run(["sessions"]);
  return lines.map((line) => String(line.session_id));
}

/** The events of `stream` in `host.db`, with the host's wall clock time of each. */
export function timedEvents(cupper: Cupper, stream: string) {
  const db = new DatabaseSync(join(cupper.dataDir, "host.db"), { readOnly: true });
  try {
    const rows = db.prepare("SELECT json FROM events WHERE stream = ? ORDER BY seq").all(stream);
    return (rows as { json: string }[]).map(
      ({ json }) =>
        JSON.parse(json) as {
          seq: number;
          type: string;
          ts: number;
          data: Record<string, unknown>;
        },
    );
  } finally {
    db.close();
  }
}

type Screened = Window & { __perfScreen?: number };

/**
 * Times `go` (which returns its own wall clock time) to the first frame with
 * rows of the message stream that were not on screen before it.
 */
export async function screenTimed(cupper: Cupper, go: () => Promise<number>): Promise<number> {
  await cupper.window.evaluate(() => {
    const rows = () => document.querySelectorAll<HTMLElement>("[data-stream-list] [data-index]");
    rows().forEach((row) => (row.dataset.perfOld = ""));
    const page = window as Screened;
    page.__perfScreen = undefined;
    const look = () => {
      if ([...rows()].some((row) => row.dataset.perfOld === undefined)) {
        page.__perfScreen = performance.timeOrigin + performance.now();
      } else requestAnimationFrame(look);
    };
    requestAnimationFrame(look);
  });
  const sent = await go();
  const shown = await cupper.window.waitForFunction(() => (window as Screened).__perfScreen, null, {
    timeout: 120_000,
  });
  return Math.round(((await shown.jsonValue()) as number) - sent);
}

/** Session `id` opened by its link (design 5.7.2), as a notification's click does; the main process's time. */
export const openLink = (cupper: Cupper, id: string) =>
  cupper.app.evaluate(({ app }, url) => {
    const at = Date.now();
    app.emit("open-url", { preventDefault: () => undefined }, url);
    return at;
  }, `cupper-e2e://session/${id}`);
