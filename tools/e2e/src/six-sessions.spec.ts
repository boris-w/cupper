// E2E-six-sessions (design 5.9.4, NFR-11): six sessions run their turns at
// once, on the one adapter of their agent; each turn ends with its own output
// only, and the window shows each session as it is opened. A load for the
// runners, not the author's machine (CLAUDE.md rule 10).

import { expect, test } from "./fixture";
import { events, untilEvent } from "./helpers";
import { openSession, session } from "./nfr1";
import { tree } from "./procs";

const SIX = [1, 2, 3, 4, 5, 6];

test("E2E-six-sessions: six turns at once, each its own output, on one adapter", async ({
  cupper,
}) => {
  await cupper.launch();
  await cupper.connected();
  const script = (n: number) => `say start-${n}; flood 1000 32; say end-${n}`;
  const streams = await Promise.all(SIX.map((n) => session(cupper, script(n))));
  for (const stream of streams) {
    const ended = await untilEvent(cupper, stream, (e) => e.type === "turn.completed");
    expect(ended.data.status, stream).toBe("completed");
  }
  for (const [i, stream] of streams.entries()) {
    const deltas = events(cupper, stream).filter((e) => e.type === "item.delta");
    const said = deltas.map((e) => String(e.data.text)).join("");
    const ends = SIX.filter((n) => said.includes(`end-${n}`));
    expect(ends, stream).toEqual([i + 1]);
  }
  // Claude's sessions share its adapter (IT-six-sessions-shared-adapter).
  const host = cupper.host();
  const agents = tree(host?.pid ?? 0).filter((proc) => proc.args.includes("cupper-fake-agent"));
  expect(agents).toHaveLength(1);
  const main = cupper.window.locator("main");
  for (const n of SIX) {
    await openSession(cupper, `say start-${n};`);
    await expect(main.getByText(`end-${n}`, { exact: true })).toBeVisible();
    const others = SIX.filter((m) => m !== n).map((m) =>
      main.getByText(`end-${m}`, { exact: true }),
    );
    for (const other of others) {
      await expect(other).toHaveCount(0);
    }
  }
});
