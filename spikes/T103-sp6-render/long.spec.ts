// SP-6 scenario 4 (T103; design 5.9.6, 5.9.7): sessions of 1,000 and 5,000
// items and one with a single turn of 2,000, seeded with the SP-1 mix. Each
// is opened by its link (timed to its first rows) and paged back through its
// history: before each page the first row on screen is noted, and after it
// its top must not move more than 1 px. Then the whole of what came in is
// scrolled from the top to the bottom, 120 px a frame (the frame times). The
// renderer's memory after opening and after the scroll.

import { expect, test, type Cupper } from "../../tools/e2e/src/fixture";
import { texts, untilEvent } from "../../tools/e2e/src/helpers";
import { session } from "../../tools/e2e/src/nfr1";
import { openTimed, record, seed, startFrames, stopFrames } from "./measure";

/** At most, for a session whose history would not end. */
const PAGES = 60;
type Paging = Window & { __sp6Anchor?: { row: Element; top: number } };

/** The renderer's working set and its JS heap, in MB. */
async function memory(cupper: Cupper) {
  const tab = await cupper.app.evaluate(({ app }) =>
    app.getAppMetrics().find((m) => m.type === "Tab")?.memory.workingSetSize,
  );
  const heap = await cupper.window.evaluate(
    () => (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory?.usedJSHeapSize,
  );
  return { renderer_mb: Math.round((tab ?? 0) / 1024), heap_mb: Math.round((heap ?? 0) / 2 ** 20) };
}

/** From the top of the list to its bottom, `step` px a frame. */
const scrollDown = (cupper: Cupper, step: number) =>
  cupper.window.evaluate(async (step) => {
    const list = document.querySelector<HTMLElement>("[data-stream-list]");
    if (!list) return;
    list.scrollTop = 0;
    await new Promise((done) => setTimeout(done, 500));
    await new Promise<void>((done) => {
      const tick = () => {
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
    const list = document.querySelector<HTMLElement>("[data-stream-list]");
    const top = list?.getBoundingClientRect().top ?? 0;
    const rows = [...document.querySelectorAll<HTMLElement>("[data-stream-list] [data-index]")]
      .filter((row) => row.textContent !== more)
      .sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top);
    const row = rows.find((r) => r.getBoundingClientRect().bottom > top);
    if (row) (window as Paging).__sp6Anchor = { row, top: row.getBoundingClientRect().top };
  }, more);

/** How far the noted row moved, once the page is in and laid out. */
const anchorMoved = (cupper: Cupper) =>
  cupper.window.evaluate(async () => {
    await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
    const anchor = (window as Paging).__sp6Anchor;
    if (!anchor?.row.isConnected) return null;
    return Math.abs(anchor.row.getBoundingClientRect().top - anchor.top);
  });

async function measure(cupper: Cupper, name: string, id: string) {
  const t = await texts(cupper);
  const first = await openTimed(cupper, id);
  const opened = await memory(cupper);
  const more = t("stream:history_more");
  const list = cupper.window.locator("[data-stream-list]");
  const moved: (number | null)[] = [];
  for (let page = 0; page < PAGES; page++) {
    await list.evaluate((el) => (el.scrollTop = 0));
    // At the top, the list renders the history control in a frame or two.
    const button = list.getByRole("button", { name: more });
    const shown = await button.waitFor({ timeout: 5000 }).then(() => true, () => false);
    if (!shown) break;
    await noteAnchor(cupper, more);
    await button.click();
    await expect(list.getByRole("button", { name: t("stream:history_loading") })).toHaveCount(0);
    moved.push(await anchorMoved(cupper));
  }
  await startFrames(cupper);
  await scrollDown(cupper, 120);
  const scroll = await stopFrames(cupper);
  const scrolled = await memory(cupper);
  record("4-long", { session: name, first_render_ms: first, pages: moved.length, anchor_moved_px: moved, scroll, opened, scrolled });
}

test("SP-6 scenario 4: long sessions opened, scrolled and paged back", async ({ cupper }) => {
  test.setTimeout(40 * 60_000);
  await cupper.launch();
  await cupper.connected();
  const made = await session(cupper, "say hi");
  await untilEvent(cupper, made, (event) => event.type === "turn.completed");
  const [thousand] = await seed(cupper, { sessions: 1, turns: 20, items_per_turn: 50 });
  const [fiveThousand] = await seed(cupper, { sessions: 1, turns: 100, items_per_turn: 50 });
  const [large] = await seed(cupper, { sessions: 1, turns: 0, items_per_turn: 0, large_turn_items: 2000 });
  for (const [name, id] of [["1000", thousand], ["5000", fiveThousand], ["one turn of 2000", large]] as const) {
    expect(id, name).toBeDefined();
    await measure(cupper, name, id as string);
  }
});
