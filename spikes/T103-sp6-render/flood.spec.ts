// SP-6 scenario 2 (T103; design 5.9.6, 5.9.7): one session on screen streams
// `flood_rate 200 120 67` (fragments of an SP-1 message's length) while keys
// go into the composer every 200 ms. Measured: the frame times, the input
// latency (from `sendInputEvent` in the main process to the first frame whose
// composer holds the character), each process's CPU, and the output latency.
//
// The fake agent's fragments carry no time, so the output latency is taken
// against the host's events: the renderer reads, each frame, how much of the
// flood the message on screen holds (fragment i is one letter, i % 26, 67
// times over, so the text is one run of letters); an `item.delta` counts as
// on screen in the first frame that holds all the text up to it, and its
// latency runs from the event's `ts`. That leaves out the up to 40 ms the
// host gathers fragments before it writes the event. Reading the text each
// frame costs the renderer some time of its own.

import { expect, test, type Cupper } from "../../tools/e2e/src/fixture";
import { untilEvent } from "../../tools/e2e/src/helpers";
import { focusWindow } from "../../tools/e2e/src/input";
import { id, openSession, session } from "../../tools/e2e/src/nfr1";
import { record, sampleUsage, startFrames, stats, stopFrames, stored } from "./measure";

const SECONDS = 120;
const KEYS_EVERY_MS = 200;
type Shown = Window & { __sp6Text?: [number, number][]; __sp6Keys?: [number, number][] };

/** Each frame, how long the run of letters in the last row is, with the frame's time. */
const watchText = (cupper: Cupper) =>
  cupper.window.evaluate(() => {
    const seen: [number, number][] = [];
    (window as Shown).__sp6Text = seen;
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
    (window as Shown).__sp6Keys = seen;
    const box = document.querySelector<HTMLTextAreaElement>("[data-composer] textarea");
    const tick = () => {
      const length = box?.value.length ?? 0;
      if (length > (seen.at(-1)?.[1] ?? 0)) seen.push([performance.timeOrigin + performance.now(), length]);
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });

/** Types one "a" as real input; the main process's time just before. */
async function key(cupper: Cupper): Promise<number> {
  const window = await cupper.app.browserWindow(cupper.window);
  return window.evaluate((w) => {
    const at = Date.now();
    w.webContents.sendInputEvent({ type: "keyDown", keyCode: "a" });
    w.webContents.sendInputEvent({ type: "char", keyCode: "a" });
    w.webContents.sendInputEvent({ type: "keyUp", keyCode: "a" });
    return at;
  });
}

test("SP-6 scenario 2: one session's flood while typing", { tag: "@focus" }, async ({ cupper }) => {
  test.setTimeout((SECONDS + 300) * 1000);
  await cupper.launch();
  const { pid } = await cupper.connected();
  const stream = await session(cupper, "say s2");
  const said = await untilEvent(cupper, stream, (e) => e.type === "turn.completed");
  await openSession(cupper, "say s2");
  await focusWindow(cupper);
  await cupper.window.locator("[data-composer] textarea").focus();
  await cupper.cli.ok(["send", id(stream), `flood_rate 200 ${SECONDS} 67`]);
  await untilEvent(cupper, stream, (e) => e.type === "item.delta" && e.seq > said.seq);

  await Promise.all([startFrames(cupper), watchText(cupper), watchKeys(cupper)]);
  const usage = sampleUsage(cupper, pid as number);
  const sent: number[] = [];
  const end = Date.now() + (SECONDS - 20) * 1000;
  while (Date.now() < end) {
    sent.push(await key(cupper));
    await cupper.window.waitForTimeout(KEYS_EVERY_MS);
  }
  const frames = await stopFrames(cupper);
  const cpu = usage();
  const { text, keys } = await cupper.window.evaluate(() => ({
    text: (window as Shown).__sp6Text ?? [],
    keys: (window as Shown).__sp6Keys ?? [],
  }));

  // The k-th key is in once the composer holds k characters more than before it.
  const base = (keys[0]?.[1] ?? 1) - 1;
  const input = sent.flatMap((at, k) => {
    const shown = keys.find(([, length]) => length - base >= k + 1);
    return shown ? [shown[0] - at] : [];
  });
  // The flood's message: the item whose deltas carry the most text.
  const deltas = stored(cupper, stream).filter((e) => e.type === "item.delta" && e.seq > said.seq);
  const sizes = new Map<unknown, number>();
  for (const e of deltas) sizes.set(e.data.item_id, (sizes.get(e.data.item_id) ?? 0) + String(e.data.text).length);
  const flood = [...sizes].sort((a, b) => b[1] - a[1])[0]?.[0];
  let total = 0;
  const output = deltas
    .filter((e) => e.data.item_id === flood)
    .flatMap((e) => {
      total += String(e.data.text).length;
      const shown = text.find(([at, length]) => length >= total && at >= e.ts);
      return shown && e.ts >= (text[0]?.[0] ?? Infinity) ? [shown[0] - e.ts] : [];
    });
  expect(input.length, "keys that showed").toBeGreaterThan(0);
  record("2-flood", {
    frames,
    input_latency: stats(input),
    keys: { sent: sent.length, shown: input.length },
    output_latency: stats(output),
    flood_chars_shown: text.at(-1)?.[1] ?? 0,
    flood_chars_written: total,
    usage: cpu,
  });
});
