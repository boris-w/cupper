// E2E-notification-navigate (design 5.2.9, 5.7.8): the host only records
// its notifications here (`test_config.notifier: record`); a click on one,
// through `test/notification_click`, goes on the control connection as
// `ui/navigate`, and the window shows that session. The request waiting there
// shows its panel, which takes no focus: navigation never moves it there.
// Needs no window focus. E2E-presence-suppresses-notification (design 5.2.9):
// what the window in front shows is not notified of; it needs the focus.

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Cupper } from "./fixture";
import { events, untilEvent } from "./helpers";
import { focusWindow, requestPanel } from "./input";
import { dial, initialize, keyProof } from "./raw";

/** `method` on the test's own connection, as `test_harness`. */
async function asHarness(cupper: Cupper, method: string, params: unknown) {
  const peer = await dial(cupper.dataDir);
  const key = Buffer.from(readFileSync(join(cupper.root, "harness_key"), "utf8").trim(), "hex");
  const init = await initialize(peer, keyProof("harness", key, peer.nonce));
  expect(init.error).toBeUndefined();
  const { error } = await peer.request(2, method, params);
  expect(error, method).toBeUndefined();
  peer.socket.destroy();
}

test("E2E-notification-navigate: a click on a notification opens its session, its request panel without the focus", async ({
  cupper,
}) => {
  await cupper.launch();
  await cupper.connected();
  const script = "permission k1";
  const project = ["--agent", "claude", "--project", cupper.repo];
  const { session_id } = await cupper.cli.ok(["new", ...project, script]);
  const row = cupper.window.getByRole("button", { name: new RegExp(script) });
  await expect(row).toBeVisible();
  await expect(row).not.toHaveAttribute("aria-current", "page");

  // The window shows another page, so the host notifies of the request.
  const intent = await untilEvent(
    cupper,
    "index",
    (e) =>
      e.type === "notification.intent" &&
      e.data.session_id === session_id &&
      e.data.reason === "needs_action",
  );
  await asHarness(cupper, "test/notification_click", { notification_id: intent.data.intent_id });

  await expect(row).toHaveAttribute("aria-current", "page");
  const panel = requestPanel(cupper);
  await expect(panel).toBeVisible();
  await expect(panel).toHaveAttribute("data-activation", "manual");
  const inPanel = () =>
    cupper.window.evaluate(() => Boolean(document.activeElement?.closest("[data-request-panel]")));
  expect(await inPanel()).toBe(false);
});

test(
  "E2E-presence-suppresses-notification: no notification of the session the window shows in front; one once it shows another",
  { tag: "@focus" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    const project = ["--agent", "claude", "--project", cupper.repo];
    const watched = await cupper.cli.ok(["new", ...project, "say watched"]);
    const session_id = String(watched.session_id);
    await cupper.cli.ok(["new", ...project, "say other"]);
    const index = () => events(cupper, "index");
    const notified = (after: number) =>
      index().filter(
        (e) =>
          e.type === "notification.intent" &&
          e.data.session_id === session_id &&
          e.data.reason === "turn_completed" &&
          e.seq > after,
      );
    // Its first turn, with no window on it, is notified of: all before is written.
    const first = await untilEvent(
      cupper,
      "index",
      (e) => e.type === "notification.intent" && e.data.session_id === session_id,
    );
    await cupper.window.getByRole("button", { name: /say watched/ }).click();
    await focusWindow(cupper);
    await expect.poll(() => cupper.window.evaluate(() => document.hasFocus())).toBe(true);

    // Watched in front: its next turn ends, written with no intent. It runs
    // past the index's 100 ms of gathering, so that "running" is written too.
    await cupper.cli.ok(["send", session_id, "sleep 500; say again", "--wait"]);
    await expect
      .poll(() => {
        const summaries = index().filter(
          (e) =>
            e.type === "session.upserted" && e.data.session_id === session_id && e.seq > first.seq,
        );
        const ran = summaries.findIndex((e) => e.data.status === "running");
        return ran !== -1 && summaries.slice(ran + 1).some((e) => e.data.status !== "running");
      }, "the turn's end written")
      .toBe(true);
    expect(notified(first.seq)).toEqual([]);

    // Showing another session, the window keeps nothing away from this one.
    await cupper.window.getByRole("button", { name: /say other/ }).click();
    await cupper.cli.ok(["send", session_id, "say once more", "--wait"]);
    await expect.poll(() => notified(first.seq).length).toBe(1);
  },
);
