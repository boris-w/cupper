// PERF-ui-stream (design 5.9.6, 5.9.7 SP-6 scenarios 2 and 3; T103, T155),
// recorded only. Scenario 2: one session on screen streams `flood_rate 200
// 120 67` (fragments of an SP-1 message's length) after a code block in the
// same message, while keys go into the composer every 200 ms: frame times,
// input latency (from `sendInputEvent` in the main process to the first frame
// whose composer holds the key), output latency, each process's CPU and the
// renderer's memory. Scenario 3: four sessions the window holds stream the
// bridge gate's load, with a 5 MB blob at 5 s and a resync of some 5,000
// events at 10 s, then six as a stress report: frame times and stream resets.
//
// Output latency: the fragments carry no time, so it runs from each
// `item.delta`'s `ts` to the first frame whose message holds all the text up
// to it; fragment i is one letter, i % 26, 67 times over, so the flood is the
// one run of letters in the message. It leaves out the up to 40 ms the host
// gathers fragments before it writes. Reading the text costs the renderer a
// little each frame.

import { randomBytes } from "node:crypto";

import { expect, test, type Cupper } from "./fixture";
import { untilEvent } from "./helpers";
import { focusWindow } from "./input";
import { id, openSession, session } from "./nfr1";
import {
  harness,
  perfSamples,
  putBlob,
  sampleUsage,
  seed,
  startFrames,
  stopFrames,
  timedEvents,
} from "./perf";

const SECONDS = 120;
const CODE = "say The code:\\n\\n```rust\\nfn main() {\\n    let answer = 42\\n}\\n```\\n\\n";
type Shown = Window & { __perfText?: [number, number][]; __perfKeys?: [number, number][] };
const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));

/** Each frame in which the longest run of letters in the last row changed: its time and length. */
const watchText = (cupper: Cupper) =>
  cupper.window.evaluate(() => {
    const seen: [number, number][] = [];
    (window as Shown).__perfText = seen;
    let last = 0;
    const tick = () => {
      const rows = [...document.querySelectorAll<HTMLElement>("[data-stream-list] [data-index]")];
      const row = rows.sort((a, b) => Number(b.dataset.index) - Number(a.dataset.index))[0];
      const runs = row?.textContent?.match(/[a-z]{67,}/g) ?? [];
      const length = Math.max(0, ...runs.map((run) => run.length));
      if (length !== last) seen.push([performance.timeOrigin + performance.now(), length]);
      last = length;
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });

/** Each frame in which the composer got longer: its time and length. */
const watchKeys = (cupper: Cupper) =>
  cupper.window.evaluate(() => {
    const seen: [number, number][] = [];
    (window as Shown).__perfKeys = seen;
    const box = document.querySelector<HTMLTextAreaElement>("[data-composer] textarea");
    const tick = () => {
      const length = box?.value.length ?? 0;
      if (length > (seen.at(-1)?.[1] ?? 0))
        seen.push([performance.timeOrigin + performance.now(), length]);
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });

/** One "a" as real input; the main process's time just before. */
async function key(cupper: Cupper): Promise<number> {
  const window = await cupper.app.browserWindow(cupper.window);
  return window.evaluate((w) => {
    const at = Date.now();
    for (const type of ["keyDown", "char", "keyUp"] as const) {
      w.webContents.sendInputEvent({ type, keyCode: "a" });
    }
    return at;
  });
}

test(
  "PERF-ui-stream: one session's flood after a code block, while typing",
  { tag: ["@perf", "@focus"] },
  async ({ cupper }) => {
    test.setTimeout((SECONDS + 240) * 1000);
    await cupper.launch();
    const { pid } = await cupper.connected();
    const stream = await session(cupper, "say s2");
    const said = await untilEvent(cupper, stream, (e) => e.type === "turn.completed");
    await openSession(cupper, "say s2");
    await focusWindow(cupper);
    await cupper.window.locator("[data-composer] textarea").focus();
    await cupper.cli.ok(["send", id(stream), `${CODE}; flood_rate 200 ${SECONDS} 67`]);
    await untilEvent(cupper, stream, (e) => e.type === "item.delta" && e.seq > said.seq);

    await Promise.all([startFrames(cupper), watchText(cupper), watchKeys(cupper)]);
    const stopUsage = await sampleUsage(cupper, pid as number);
    const sent: number[] = [];
    for (const end = Date.now() + (SECONDS - 20) * 1000; Date.now() < end;) {
      sent.push(await key(cupper));
      await sleep(200);
    }
    const frames = await stopFrames(cupper);
    const { cpu, memory } = stopUsage();
    const { text, keys } = await cupper.window.evaluate(() => ({
      text: (window as Shown).__perfText ?? [],
      keys: (window as Shown).__perfKeys ?? [],
    }));
    // The k-th key is in once the composer holds k more characters than before the first.
    const base = (keys[0]?.[1] ?? 1) - 1;
    const input = sent.flatMap((at, k) => {
      const shown = keys.find(([, length]) => length - base >= k + 1);
      return shown ? [shown[0] - at] : [];
    });
    // The flood's message: the item whose deltas carry the most text.
    const deltas = timedEvents(cupper, stream).filter(
      (e) => e.type === "item.delta" && e.seq > said.seq,
    );
    const sizes = new Map<unknown, number>();
    for (const e of deltas)
      sizes.set(e.data.item_id, (sizes.get(e.data.item_id) ?? 0) + String(e.data.text).length);
    const flood = [...sizes].sort((a, b) => b[1] - a[1])[0]?.[0];
    const runs = deltas.filter((e) => e.data.item_id === flood);
    const output: number[] = [];
    let total = 0;
    for (const e of runs) {
      // The flood's letters only, not the code block before it.
      total += (String(e.data.text).match(/[a-z]{67,}/g) ?? []).join("").length;
      const shown = text.find(([at, length]) => length >= total && at >= e.ts);
      if (shown && e.ts >= (text[0]?.[0] ?? Infinity)) output.push(shown[0] - e.ts);
    }
    expect(input.length, "keys that showed").toBeGreaterThan(0);
    perfSamples("ui_stream_frame", "ms", frames);
    perfSamples("ui_stream_input", "ms", input);
    perfSamples("ui_stream_output", "ms", output);
    for (const [kind, shares] of cpu)
      perfSamples(`ui_stream_cpu_${kind.toLowerCase()}`, "%core", shares);
    perfSamples("ui_stream_renderer_memory", "MB", [memory.get("Tab") ?? 0]);
  },
);

/** Scenario 3: `streams` sessions at the gate's load, a blob at 5 s and a resync at 10 s, for 20 s. */
async function streams(cupper: Cupper, count: number) {
  await cupper.launch();
  await cupper.connected();
  const host = await harness(cupper);
  const [sessions, said]: [string[], number[]] = [[], []];
  for (let n = 1; n <= count; n++) {
    const made = await session(cupper, `say g${n}`);
    said.push((await untilEvent(cupper, made, (e) => e.type === "turn.completed")).seq);
    sessions.push(made);
  }
  const [seeded] = await seed(cupper, {
    sessions: 1,
    turns: 20,
    items_per_turn: 95,
    mix: "sp1_code",
  });
  const blob = await putBlob(host.peer, host.call, randomBytes(5 * 1024 * 1024));
  // The window holds the session on screen and the three seen before it (design 5.7.6).
  for (let n = Math.max(1, count - 3); n <= count; n++) await openSession(cupper, `say g${n}`);
  const more = sessions.slice(0, -4);
  await cupper.window.evaluate((more) => more.forEach((s) => window.__cupperGate.follow(s)), more);
  for (const s of sessions) await cupper.cli.ok(["send", id(s), "flood_rate 100 50 500"]);
  for (const [n, s] of sessions.entries()) {
    await untilEvent(cupper, s, (e) => e.type === "item.delta" && e.seq > (said[n] ?? 0));
  }
  const resets = await cupper.window.evaluate(() => window.__cupperGate.resets());
  await startFrames(cupper);
  const started = Date.now();
  await sleep(5000);
  const got = await cupper.window.evaluate((hash) => window.__cupperGate.getBlob(hash), blob);
  await sleep(10_000 - (Date.now() - started));
  const resync = await cupper.window.evaluate(
    (s) => window.__cupperGate.resync(s),
    `session/${seeded}`,
  );
  await sleep(20_000 - (Date.now() - started));
  const frames = await stopFrames(cupper);
  const reset = (await cupper.window.evaluate(() => window.__cupperGate.resets())) - resets;
  expect(got.bytes).toBe(5 * 1024 * 1024);
  expect(resync.events).toBeGreaterThanOrEqual(5000);
  perfSamples(`ui_streams${count}_frame`, "ms", frames);
  perfSamples(`ui_streams${count}_resets`, "resets", [reset]);
}

for (const count of [4, 6]) {
  test(
    `PERF-ui-stream: ${count} streams at once, a blob and a resync${count > 4 ? " (stress report)" : ""}`,
    { tag: "@perf" },
    async ({ cupper }) => {
      test.setTimeout(10 * 60_000);
      await streams(cupper, count);
    },
  );
}
