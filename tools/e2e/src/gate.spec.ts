// PERF-bridge-gate (T078, design 5.7.1): the main process's event loop delay
// while it forwards the judging load to a real renderer. The window holds the
// index and four sessions that stream, through the UI's own subscriptions,
// and on top of that takes a 5 MB blob and a resync of some 5,000 events;
// the index gets three new sessions every 200 ms, about 1 KB a frame. Judged with the window on
// screen and covered: p99 under 5 ms, no stream reset. Six streams of 200
// fragments a second follow as a stress report, not judged. Each test logs
// its numbers as one `gate` line; when `CUPPER_PERF_OUT` names a file (the
// e2e job outside `ci pr`), a judged test that passed adds its p99 there as a
// PERF sample, for the trend in `testdata/perf/history.jsonl`. Tagged `@perf`:
// a GitHub-hosted runner is no reference machine and leaves them out (T197).

import { createHash, randomBytes } from "node:crypto";
import { appendFileSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Cupper } from "./fixture";
import { id, openSession, session } from "./nfr1";
import { dial, initialize, keyProof, type Peer } from "./raw";

declare global {
  interface Window {
    /** e2e builds only (`apps/desktop/src/renderer/gate.ts`). */
    __cupperGate: {
      count(): void;
      follow(stream: string): void;
      tallies(): Record<string, { frames: number; events: number; sampled: number; bytes: number }>;
      resets(): number;
      getBlob(hash: string): Promise<{ bytes: number; ms: number }>;
      resync(stream: string): Promise<{ events: number; head: number; ms: number }>;
    };
  }
}

/** The measured stretch, after the streams are up. */
const SECONDS = 20;
const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));

/** The test's own connection to the host, as `test_harness`, and a way to call on it. */
async function harness(cupper: Cupper) {
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
async function putBlob(
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

/** Starts the main process's event loop delay histogram and CPU clock. */
const startClock = (cupper: Cupper) =>
  cupper.app.evaluate(() => {
    const { monitorEventLoopDelay } = process.getBuiltinModule("node:perf_hooks");
    const delay = monitorEventLoopDelay({ resolution: 1 });
    delay.enable();
    const clock = { delay, cpu: process.cpuUsage(), at: performance.now() };
    Object.assign(globalThis, { __gateClock: clock });
  });

/** The delay's p50, p99 and max in ms, and the share of one core used, since the start. */
const stopClock = (cupper: Cupper) =>
  cupper.app.evaluate(() => {
    interface Clock {
      delay: import("node:perf_hooks").ELDHistogram;
      cpu: NodeJS.CpuUsage;
      at: number;
    }
    const { delay, cpu, at } = (globalThis as unknown as { __gateClock: Clock }).__gateClock;
    delay.disable();
    const used = process.cpuUsage(cpu);
    const ms = (ns: number) => Math.round(ns / 1e4) / 100;
    const core = (used.user + used.system) / 1000 / (performance.now() - at);
    const [p50, p99, max] = [delay.percentile(50), delay.percentile(99), delay.max].map(ms);
    return { p50, p99, max, core: Math.round(core * 100) / 100 };
  });

/**
 * Covers the app's window with another one, and stops its animation frames.
 * Under Playwright, Chromium keeps the page visible and its frames running
 * even covered, minimized or hidden (T078), so the frames are stopped by
 * hand: what a hidden page loses that the store cares about (design 5.7.1).
 */
async function cover(cupper: Cupper) {
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
  await cupper.window.evaluate(() => {
    window.requestAnimationFrame = () => 0;
  });
}

/** The page's animation frames in one second. */
const frameRate = (cupper: Cupper) =>
  cupper.window.evaluate(
    () =>
      new Promise<number>((done) => {
        let frames = 0;
        const tick = () => void (frames++, requestAnimationFrame(tick));
        requestAnimationFrame(tick);
        setTimeout(() => done(frames), 1000);
      }),
  );

/**
 * The run: `streams` sessions flooding `rate` fragments of 500 characters a
 * second, the window holding the last four it opened and following the rest
 * through the gate; the blob at 5 s, the resync at 10 s, the index growing
 * all along. Its numbers.
 */
async function run(cupper: Cupper, { streams = 4, rate = 100, covered = false } = {}) {
  await cupper.launch({ throttled: true });
  await cupper.connected();
  const host = await harness(cupper);
  const flood = `flood_rate ${rate} ${SECONDS + 30} 500`;
  const sessions: string[] = [];
  for (let n = 1; n <= streams; n++) sessions.push(await session(cupper, `say g${n}; ${flood}`));
  // About 5,000 events in one session, to replay (`test/seed`, design 5.9.9).
  const seed = (sessions: number, turns: number, items: number) =>
    host.call("test/seed", { profile: { sessions, turns, items_per_turn: items, mix: "sp1" } });
  await seed(1, 20, 95);
  const listed = await cupper.cli.run(["sessions"]);
  const ids = listed.lines.map((line) => String(line.session_id));
  const seeded = ids.find((one) => !sessions.map(id).includes(one));
  expect(seeded).toBeDefined();
  const blob = await putBlob(host.peer, host.call, randomBytes(5 * 1024 * 1024));
  // The UI holds the session on screen and the three seen before (design 5.7.6).
  for (let n = Math.max(1, streams - 3); n <= streams; n++) await openSession(cupper, `say g${n};`);
  await cupper.window.evaluate(
    (more) => more.forEach((s) => window.__cupperGate.follow(s)),
    sessions.slice(0, -4),
  );
  if (covered) await cover(cupper);
  const frames = await frameRate(cupper);
  const resets = await cupper.window.evaluate(() => window.__cupperGate.resets());

  await cupper.window.evaluate(() => window.__cupperGate.count());
  await startClock(cupper);
  const started = Date.now();
  const index = setInterval(() => void seed(3, 0, 0), 200);
  await sleep(5000);
  const got = await cupper.window.evaluate((hash) => window.__cupperGate.getBlob(hash), blob);
  await sleep(10_000 - (Date.now() - started));
  const resync = await cupper.window.evaluate(
    (s) => window.__cupperGate.resync(s),
    `session/${seeded}`,
  );
  await sleep(SECONDS * 1000 - (Date.now() - started));
  clearInterval(index);
  const delay = await stopClock(cupper);
  const seconds = (Date.now() - started) / 1000;
  const tallies = await cupper.window.evaluate(() => window.__cupperGate.tallies());
  const reset = (await cupper.window.evaluate(() => window.__cupperGate.resets())) - resets;
  // Frames a second and their average size, by stream: `index`, `g1`…, `resync`.
  const name = (stream: string) =>
    stream === "index"
      ? stream
      : sessions.includes(stream)
        ? `g${sessions.indexOf(stream) + 1}`
        : "resync";
  const load = Object.fromEntries(
    Object.entries(tallies).map(([stream, t]) => {
      const perSecond = Math.round((t.frames / seconds) * 10) / 10;
      return [name(stream), { perSecond, bytes: Math.round(t.bytes / (t.sampled || 1)) }];
    }),
  );
  const visibility = await cupper.window.evaluate(() => document.visibilityState);
  const numbers = {
    streams,
    rate,
    covered,
    visibility,
    frames,
    seconds,
    delay,
    reset,
    blob: got,
    resync,
    load,
  };
  console.log(`gate ${JSON.stringify(numbers)}`);
  return numbers;
}

test.describe.configure({ mode: "serial", timeout: 180_000 });

for (const covered of [false, true]) {
  test(
    `PERF-bridge-gate: the judging load, the window ${covered ? "covered, its frames stopped" : "on screen"}`,
    {
      tag: "@perf",
    },
    async ({ cupper }) => {
      const { delay, reset, blob, resync, load, frames } = await run(cupper, { covered });
      expect(frames > 0, "animation frames run unless stopped").toBe(!covered);
      expect(blob.bytes).toBe(5 * 1024 * 1024);
      expect(resync.events).toBeGreaterThanOrEqual(5000);
      expect(Object.keys(load).filter((stream) => /^g\d$/.test(stream))).toHaveLength(4);
      expect(reset, "no stream reset").toBe(0);
      expect(delay.p99, "the main process's event loop delay, p99 in ms").toBeLessThan(5);
      const out = process.env.CUPPER_PERF_OUT;
      if (out) {
        const metric = `bridge_loop_delay_p99${covered ? "_covered" : ""}`;
        appendFileSync(out, `${JSON.stringify({ metric, unit: "ms", samples: [delay.p99] })}\n`);
      }
    },
  );
}

test(
  "PERF-bridge-gate: six streams of 200 fragments a second, a stress report",
  { tag: "@perf" },
  async ({ cupper }) => {
    await run(cupper, { streams: 6, rate: 200 });
  },
);
