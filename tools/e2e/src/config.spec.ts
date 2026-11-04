// The permission button on the real app (design 5.7.4, 5.8.2; T231): full
// access only through the native confirmation, which the main process shows
// with what the host describes; once it is on, the composer says so.

import { expect, test } from "./fixture";
import { audit, texts, untilEvent } from "./helpers";

test("E2E-full-access-native: full access through the native dialog, then the composer says it is on", async ({
  cupper,
}) => {
  await cupper.launch();
  await cupper.connected();
  const t = await texts(cupper);
  const project = ["--agent", "claude", "--project", cupper.repo];
  const { session_id } = await cupper.cli.ok(["new", ...project, "say ready"]);
  const stream = `session/${String(session_id)}`;
  await untilEvent(cupper, stream, (e) => e.type === "turn.completed");
  await cupper.window.getByRole("button", { name: "say ready" }).click();
  const button = cupper.window.getByRole("button", { name: t("composer:permission") });
  await expect(button).toHaveText("Manual");

  // Cancelled first, then confirmed: only the confirmed one reaches the host.
  for (const [n, response] of [
    [0, 1],
    [1, 0],
  ] as const) {
    await button.click();
    await cupper.window.getByRole("menuitemradio", { name: /Bypass permissions/ }).click();
    const dialog = await cupper.dialogs.next(n);
    expect(dialog).toMatchObject({ message: t("privileged:set_config_title") });
    expect(dialog.detail).toContain("say ready");
    await cupper.dialogs.answer(dialog.index, response);
    // After a cancel the same action waits 2 s (design 5.7.8).
    if (response === 1) await cupper.window.waitForTimeout(2_100);
  }
  await expect(cupper.window.getByText(t("composer:full_access_on"))).toBeVisible();
  await expect(button).toHaveText("Bypass permissions");
  const set = audit(cupper, "privileged.executed").filter((d) => d.method === "session/set_config");
  expect(set).toEqual([expect.objectContaining({ action: "full_access" })]);
});
