// E2E-NFR1-5 (design 5.9.4): the host is killed and the app starts another;
// the session goes on. The new host loads the session on the first command
// for it, and writes off what the killed one left: the running turn as
// interrupted, the waiting request as expired (T089). Items left open in the
// turn are not looked at: their end is T169's.

import { expect, test, type Cupper } from "./fixture";
import { events, killHost, texts, untilEvent } from "./helpers";
import { requestPanel } from "./input";
import { id, openSession, session } from "./nfr1";
import { gone, tree } from "./procs";

/**
 * A session on screen whose turn got as far as `barrier` (an event type)
 * with `script`, when the host is killed. Returns its stream once the window
 * is connected to the next host, and the seq the script started after.
 */
async function killedAt(cupper: Cupper, script: string, barrier: string) {
  await cupper.launch();
  await cupper.connected();
  const stream = await session(cupper, "say first");
  await untilEvent(cupper, stream, (event) => event.type === "turn.completed");
  await openSession(cupper, "say first");
  const seq = events(cupper, stream).at(-1)?.seq ?? 0;
  await cupper.cli.ok(["send", id(stream), script]);
  await untilEvent(cupper, stream, (event) => event.type === barrier && event.seq > seq);
  const host = cupper.host();
  const killed = host ? tree(host.pid) : [];
  await killHost(cupper);
  // The killed host's adapter is gone too.
  expect(await gone(killed, 10_000)).toEqual([]);
  return { stream, seq };
}

/**
 * The next message: it loads the session in the new host, which writes off
 * what the killed one left, then goes through the agent's `session/resume`.
 * Returns the events from `seq` on.
 */
async function goesOn(cupper: Cupper, stream: string, seq: number) {
  const sent = await cupper.cli.run(["send", id(stream), "say again", "--wait"]);
  expect(sent.code, sent.stderr).toBe(0);
  await expect(cupper.window.locator("main").getByText("say again", { exact: true })).toBeVisible();
  return events(cupper, stream).filter((event) => event.seq > seq);
}

test(
  "E2E-NFR1-5: killed while a turn runs, the turn ends interrupted and the session goes on",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const { stream, seq } = await killedAt(cupper, "say working; wait_cancel", "item.started");
    const after = await goesOn(cupper, stream, seq);
    const ended = after.filter((event) => event.type === "turn.completed");
    expect(ended.map((event) => event.data.status)).toEqual(["interrupted", "completed"]);
    const t = await texts(cupper);
    await expect(
      cupper.window.locator("main").getByText(t("stream:turn_interrupted")),
    ).toBeVisible();
  },
);

test(
  "E2E-NFR1-5: killed while a request waits, the request expires and the session goes on",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const { stream, seq } = await killedAt(cupper, "permission p1", "interaction.requested");
    const after = await goesOn(cupper, stream, seq);
    const resolved = after.find((event) => event.type === "interaction.resolved");
    expect(resolved?.data).toMatchObject({ status: "expired", reason: "host_restarted" });
    await expect(requestPanel(cupper)).toHaveCount(0);
    const t = await texts(cupper);
    const request = String(resolved?.data.request_id);
    const record = cupper.window.locator(`[data-request-record="${request}"]`);
    await expect(record).toHaveText(t("request:record_unanswered"));
  },
);
