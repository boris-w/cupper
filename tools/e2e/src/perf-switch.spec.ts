// PERF-ui-switch (design 5.9.6, 5.9.7 SP-6 scenario 7; T103, T155), recorded
// only: from switching to a session to the first frame with its rows. Eight
// seeded sessions (5 turns of 50 items, `sp1_code`) opened by their links in
// turn: each was last seen eight switches before, past the four the window
// keeps (design 5.7.6), so none is in the LRU; two back and forth are both in
// it. Then the jump from a notification (design 5.2.9): sessions whose agent
// asks for permission while the window shows another session, whose recorded
// notification `test/notification_click` clicks, timed from the click. Last,
// "the next session that needs action" (⌘⌥A, T153) from one of those sessions
// to the next, round them all: after a first round each was last seen ten
// switches before, so none is in the LRU. The command goes to the window as
// the menu sends it, timed from the main process; the menu itself would need
// the window's focus.

import { expect, test } from "./fixture";
import { untilEvent } from "./helpers";
import { session } from "./nfr1";
import { harness, openLink, perfSamples, screenTimed, seed } from "./perf";

const RUNS = 30;
/** Each needs a session with a request waiting. */
const CLICKS = 10;

test(
  "PERF-ui-switch: sessions in and out of the LRU, from a notification, and the next that needs action",
  { tag: "@perf" },
  async ({ cupper }) => {
    test.setTimeout(30 * 60_000);
    await cupper.launch();
    await cupper.connected();
    const made = await session(cupper, "say hi");
    await untilEvent(cupper, made, (event) => event.type === "turn.completed");
    const pool = await seed(cupper, { sessions: 8, turns: 5, items_per_turn: 50, mix: "sp1_code" });
    expect(pool).toHaveLength(8);
    const open = (id: string) => screenTimed(cupper, () => openLink(cupper, id));

    const away: number[] = [];
    for (let n = 0; n < RUNS; n++) away.push(await open(pool[n % pool.length] as string));
    const [a, b] = pool as [string, string];
    await open(a);
    await open(b);
    const cached: number[] = [];
    for (let n = 0; n < RUNS; n++) cached.push(await open(n % 2 === 0 ? a : b));

    // The window shows a seeded session while the requests come: each is notified.
    const asking = [];
    for (let n = 0; n < CLICKS; n++) asking.push(await session(cupper, "permission k1"));
    const intents = [];
    for (const stream of asking) {
      const id = stream.replace("session/", "");
      const intent = await untilEvent(
        cupper,
        "index",
        (e) =>
          e.type === "notification.intent" &&
          e.data.session_id === id &&
          e.data.reason === "needs_action",
      );
      intents.push(String(intent.data.intent_id));
    }
    const host = await harness(cupper);
    const clicked: number[] = [];
    for (const intent of intents) {
      await open(a);
      clicked.push(
        await screenTimed(cupper, async () => {
          const at = Date.now();
          await host.call("test/notification_click", { notification_id: intent });
          return at;
        }),
      );
    }
    const command = (id: string) =>
      cupper.app.evaluate(({ BrowserWindow }, id) => {
        const at = Date.now();
        BrowserWindow.getAllWindows()[0]?.webContents.send("shell:command", { id });
        return at;
      }, id);
    const next: number[] = [];
    for (let n = 0; n < CLICKS + RUNS; n++) {
      const ms = await screenTimed(cupper, () => command("next_needs_action"));
      if (n >= CLICKS) next.push(ms);
    }
    perfSamples("ui_switch_not_in_lru", "ms", away);
    perfSamples("ui_switch_in_lru", "ms", cached);
    perfSamples("ui_switch_from_notification", "ms", clicked);
    perfSamples("ui_switch_next_needs_action", "ms", next);
  },
);
