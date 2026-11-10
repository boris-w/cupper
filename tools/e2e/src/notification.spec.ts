// E2E-notification-navigate (design 5.2.9, 5.7.8): the host only records
// its notifications here (`test_config.notifier: record`); a click on one,
// through `test/notification_click`, goes on the control connection as
// `ui/navigate`, and the window shows that session. The request waiting there
// shows its panel, which takes no focus: navigation never moves it there.
// Needs no window focus.

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Cupper } from "./fixture";
import { untilEvent } from "./helpers";
import { requestPanel } from "./input";
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
