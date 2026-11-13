// SP-6 scenario 3 (T103; design 5.9.6, 5.9.7): four sessions the window holds
// stream at once, the bridge gate's load (T078: 100 fragments of 500
// characters a second each), while the renderer takes a 5 MB blob at 5 s and
// replays a seeded session of some 5,000 events at 10 s; then six streams as
// a stress report. Measured over 20 s: the renderer's frame times and the
// stream resets. For T206's pauses, the gaps between each stream's events as
// the host wrote them; on a runner, where `sudo` needs no password, the
// host's `fsync`s (`fs_usage`) over the same stretch go to `results/`, to
// set the long gaps against `host.db`'s `checkpoint_fullfsync`.

import { spawn } from "node:child_process";
import { once } from "node:events";
import { randomBytes } from "node:crypto";
import { createWriteStream, mkdirSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Cupper } from "../../tools/e2e/src/fixture";
import { untilEvent } from "../../tools/e2e/src/helpers";
import { id, openSession, session } from "../../tools/e2e/src/nfr1";
import { harness, putBlob } from "../../tools/e2e/src/perf";
import { RESULTS, record, seed, startFrames, stats, stopFrames, stored } from "./measure";

const SECONDS = 20;
const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));

/** The host's file system calls with "fsync" in them, into `results/`, until stopped; none without passwordless sudo. */
function watchFsyncs(pid: number, name: string) {
  mkdirSync(RESULTS, { recursive: true });
  const out = createWriteStream(join(RESULTS, `${name}-fsyncs.txt`));
  const fsUsage = spawn("sudo", ["-n", "fs_usage", "-w", "-f", "filesys", String(pid)]);
  fsUsage.stdout.on("data", (chunk: Buffer) => {
    for (const line of chunk.toString().split("\n")) if (/fsync/i.test(line)) out.write(`${line}\n`);
  });
  fsUsage.stderr.on("data", (chunk: Buffer) => out.write(`fs_usage: ${chunk.toString()}`));
  const exited = once(fsUsage, "close").catch(() => undefined);
  // Stopped and gone before the next run's, which the kernel's trace would refuse.
  return async () => {
    fsUsage.kill("SIGINT");
    await Promise.race([exited, new Promise((done) => setTimeout(done, 10_000))]);
    out.end();
  };
}

async function run(cupper: Cupper, streams: number) {
  await cupper.launch();
  const { pid } = await cupper.connected();
  const host = await harness(cupper);
  const [sessions, said]: [string[], number[]] = [[], []];
  for (let n = 1; n <= streams; n++) {
    const made = await session(cupper, `say g${n}`);
    said.push((await untilEvent(cupper, made, (e) => e.type === "turn.completed")).seq);
    sessions.push(made);
  }
  const [seeded] = await seed(cupper, { sessions: 1, turns: 20, items_per_turn: 95 });
  const blob = await putBlob(host.peer, host.call, randomBytes(5 * 1024 * 1024));
  // The window holds the session on screen and the three seen before it
  // (design 5.7.6); the rest it follows through the gate.
  for (let n = Math.max(1, streams - 3); n <= streams; n++) await openSession(cupper, `say g${n}`);
  const more = sessions.slice(0, -4);
  await cupper.window.evaluate((more) => more.forEach((s) => window.__cupperGate.follow(s)), more);
  for (const s of sessions) await cupper.cli.ok(["send", id(s), `flood_rate 100 ${SECONDS + 30} 500`]);
  for (const [n, s] of sessions.entries()) {
    await untilEvent(cupper, s, (e) => e.type === "item.delta" && e.seq > (said[n] ?? 0));
  }

  const resets = await cupper.window.evaluate(() => window.__cupperGate.resets());
  const stopFsyncs = watchFsyncs(pid as number, `3-streams-${streams}`);
  await startFrames(cupper);
  const started = Date.now();
  await sleep(5000);
  const got = await cupper.window.evaluate((hash) => window.__cupperGate.getBlob(hash), blob);
  await sleep(10_000 - (Date.now() - started));
  const resync = await cupper.window.evaluate((s) => window.__cupperGate.resync(s), `session/${seeded}`);
  await sleep(SECONDS * 1000 - (Date.now() - started));
  const frames = await stopFrames(cupper);
  const ended = Date.now();
  await stopFsyncs();
  const reset = (await cupper.window.evaluate(() => window.__cupperGate.resets())) - resets;

  // T206: how long each stream went without an event while the host wrote them.
  const gaps = sessions.map((s) => {
    const times = stored(cupper, s)
      .filter((e) => e.ts >= started && e.ts <= ended)
      .map((e) => e.ts);
    const between = times.slice(1).map((t, i) => t - (times[i] ?? t));
    const long = times.slice(1).flatMap((t, i) => (t - (times[i] ?? t) > 1000 ? [[times[i], t]] : []));
    return { events: times.length, gap: stats(between), over_1s: long };
  });
  expect(got.bytes).toBe(5 * 1024 * 1024);
  record("3-four", { streams, frames, reset, blob: got, resync, gaps, window: [started, ended] });
}

test.describe.configure({ mode: "serial", timeout: 15 * 60_000 });

for (const streams of [4, 6]) {
  test(`SP-6 scenario 3: ${streams} streams, a blob and a resync`, async ({ cupper }) => {
    await run(cupper, streams);
  });
}
