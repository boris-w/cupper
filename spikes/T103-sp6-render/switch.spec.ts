// SP-6 scenario 7 (T103; design 5.9.6, 5.9.7): switching sessions, timed from
// the session's link in the main process (the way a notification's click
// goes, design 5.7.2) to the first frame with its rows. Eight seeded sessions
// (5 turns of 50 items, SP-1 mix): visited in turn, each one was last seen
// eight switches before, past the four the window keeps (design 5.7.6), so
// none is in the LRU; going back and forth between two, both are. The jump
// from a notification is T155's. "The next session that needs action" is
// in the menu (`next_needs_action`) but no part of the UI handles it yet, so
// it cannot be timed (in the report).

import { expect, test } from "../../tools/e2e/src/fixture";
import { untilEvent } from "../../tools/e2e/src/helpers";
import { session } from "../../tools/e2e/src/nfr1";
import { openTimed, record, seed, stats } from "./measure";

const RUNS = Number(process.env.SP6_RUNS ?? 30);

test("SP-6 scenario 7: switching to sessions in and out of the LRU", async ({ cupper }) => {
  test.setTimeout(30 * 60_000);
  await cupper.launch();
  await cupper.connected();
  const made = await session(cupper, "say hi");
  await untilEvent(cupper, made, (event) => event.type === "turn.completed");
  const pool = await seed(cupper, { sessions: 8, turns: 5, items_per_turn: 50 });
  expect(pool).toHaveLength(8);
  const away: number[] = [];
  for (let n = 0; n < RUNS; n++) away.push(await openTimed(cupper, pool[n % pool.length] as string));
  const [a, b] = pool as [string, string];
  await openTimed(cupper, a);
  await openTimed(cupper, b);
  const cached: number[] = [];
  for (let n = 0; n < RUNS; n++) cached.push(await openTimed(cupper, n % 2 === 0 ? a : b));
  record("7-switch", { not_in_lru: stats(away), in_lru: stats(cached), samples: { away, cached } });
});
