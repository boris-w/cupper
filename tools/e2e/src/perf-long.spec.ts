// PERF-ui-long-session (design 5.9.6, 5.9.7 SP-6 scenario 4; T103, T155),
// recorded only: sessions of 1,000 and 5,000 items and one with a single turn
// of 2,000, seeded with `sp1_code` (code blocks for the highlighting, T143;
// every turn ended, so folded, T146). Each is opened by its link and timed to
// its first rows, then paged back through its history: before each page the
// first row on screen is noted, after it its top should not move more than
// 1 px. Then what came in is scrolled from the top to the bottom, 120 px a
// frame: the frame times and the renderers' memory. Each move of the list
// comes after a wheel event: only the user leaves the bottom, a move within
// 500 ms of their wheel (T245); any other the list undoes.

import { expect, test, type Cupper } from "./fixture";
import { texts, untilEvent } from "./helpers";
import { session } from "./nfr1";
import { openLink, perfSamples, screenTimed, seed, startFrames, stopFrames } from "./perf";

/** At most, for a history that would not end. */
const PAGES = 60;
type Paging = Window & { __perfAnchor?: { row: Element; top: number } };

/** The renderers' working set, in MB. */
const rendererMb = (cupper: Cupper) =>
  cupper.app.evaluate(({ app }) =>
    Math.round(
      app
        .getAppMetrics()
        .filter((m) => m.type === "Tab")
        .reduce((kb, m) => kb + m.memory.workingSetSize, 0) / 1024,
    ),
  );

/** From the top of the list to its bottom, `step` px a frame, each after a wheel event. */
const scrollDown = (cupper: Cupper, step: number) =>
  cupper.window.evaluate(async (step) => {
    const list = document.querySelector<HTMLElement>("[data-stream-list]");
    if (!list) return;
    const wheel = (deltaY: number) =>
      list.dispatchEvent(new WheelEvent("wheel", { deltaY, bubbles: true }));
    wheel(-1);
    list.scrollTop = 0;
    await new Promise((done) => setTimeout(done, 300));
    await new Promise<void>((done) => {
      const tick = () => {
        wheel(step);
        list.scrollTop += step;
        if (list.scrollTop + list.clientHeight >= list.scrollHeight - 1) done();
        else requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
  }, step);

/** Notes the first row on screen below the history control, and its top. */
const noteAnchor = (cupper: Cupper, more: string) =>
  cupper.window.evaluate((more) => {
    const top = document.querySelector("[data-stream-list]")?.getBoundingClientRect().top ?? 0;
    const rows = [...document.querySelectorAll<HTMLElement>("[data-stream-list] [data-index]")]
      .filter((row) => row.textContent !== more)
      .sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top);
    const row = rows.find((r) => r.getBoundingClientRect().bottom > top);
    if (row) (window as Paging).__perfAnchor = { row, top: row.getBoundingClientRect().top };
  }, more);

/** How far the noted row moved, two frames after the page came in. */
const anchorMoved = (cupper: Cupper) =>
  cupper.window.evaluate(async () => {
    await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
    const anchor = (window as Paging).__perfAnchor;
    return anchor?.row.isConnected
      ? Math.abs(anchor.row.getBoundingClientRect().top - anchor.top)
      : null;
  });

async function measure(cupper: Cupper, name: string, id: string) {
  const t = await texts(cupper);
  const first = await screenTimed(cupper, () => openLink(cupper, id));
  const more = t("stream:history_more");
  const list = cupper.window.locator("[data-stream-list]");
  const moved: number[] = [];
  for (let page = 0; page < PAGES; page++) {
    await list.evaluate((el) => {
      el.dispatchEvent(new WheelEvent("wheel", { deltaY: -1, bubbles: true }));
      el.scrollTop = 0;
    });
    // At the top, the list renders the history control in a frame or two.
    const button = list.getByRole("button", { name: more });
    if (
      !(await button.waitFor({ timeout: 5000 }).then(
        () => true,
        () => false,
      ))
    )
      break;
    await noteAnchor(cupper, more);
    await button.click();
    await expect(list.getByRole("button", { name: t("stream:history_loading") })).toHaveCount(0);
    const px = await anchorMoved(cupper);
    if (px !== null) moved.push(px);
  }
  await startFrames(cupper);
  await scrollDown(cupper, 120);
  perfSamples(`ui_long${name}_first_rows`, "ms", [first]);
  perfSamples(`ui_long${name}_scroll_frame`, "ms", await stopFrames(cupper));
  perfSamples(`ui_long${name}_anchor_moved`, "px", moved);
  perfSamples(`ui_long${name}_renderer_memory`, "MB", [await rendererMb(cupper)]);
}

test(
  "PERF-ui-long-session: long sessions opened, paged back and scrolled",
  { tag: "@perf" },
  async ({ cupper }) => {
    test.setTimeout(30 * 60_000);
    await cupper.launch();
    await cupper.connected();
    const made = await session(cupper, "say hi");
    await untilEvent(cupper, made, (event) => event.type === "turn.completed");
    const profile = { sessions: 1, items_per_turn: 50, mix: "sp1_code" } as const;
    const [thousand] = await seed(cupper, { ...profile, turns: 20 });
    const [fiveThousand] = await seed(cupper, { ...profile, turns: 100 });
    const [large] = await seed(cupper, { ...profile, turns: 0, large_turn_items: 2000 });
    const sessions = [
      ["1000", thousand],
      ["5000", fiveThousand],
      ["_turn2000", large],
    ] as const;
    for (const [name, id] of sessions) {
      expect(id, name).toBeDefined();
      await measure(cupper, name, id as string);
    }
  },
);
