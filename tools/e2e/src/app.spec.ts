// The app's own lifecycle (design 5.7.1, 5.7.2, NFR-8; T007's handoff): one
// instance per data directory, and quitting with work in flight asks first.

import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { createI18n } from "@cupper/i18n";

import { expect, test, type Cupper } from "./fixture";
import { BUILT } from "./paths";
import { alive, tree } from "./procs";

test("E2E-single-instance: a second launch quits and brings the first window forward", async ({
  cupper,
}) => {
  await cupper.launch();
  const { pid } = await cupper.connected();
  const visible = () =>
    cupper.app.evaluate(({ BrowserWindow }) =>
      BrowserWindow.getAllWindows().map((w) => w.isVisible()),
    );
  await cupper.app.evaluate(({ BrowserWindow }) =>
    BrowserWindow.getAllWindows().forEach((w) => w.hide()),
  );
  expect(await visible()).toEqual([false]);

  const env = { ...process.env, CUPPER_E2E_DIR: cupper.root };
  const second = spawn(BUILT.app, [], { env, stdio: "ignore" });
  try {
    const code = await new Promise((done) => second.once("close", done));
    expect(code).toBe(0);
  } finally {
    second.kill("SIGKILL");
  }
  await expect.poll(visible).toEqual([true]);
  expect(readFileSync(join(cupper.root, "logs/main.log"), "utf8")).toContain(
    "another instance is running",
  );
  // Still the same host, and no second one.
  expect((await cupper.cli.ok(["status"])).pid).toBe(pid);
});

/** A session whose turn runs until it is cancelled; once the host counts it. */
async function runningTurn(cupper: Cupper) {
  await cupper.cli.ok(["new", "--agent", "claude", "--project", cupper.repo, "wait_cancel"]);
  await expect.poll(async () => (await cupper.cli.ok(["status"])).running_turns).toBe(1);
}

const quit = (cupper: Cupper) => cupper.app.evaluate(({ app }) => app.quit());

test("E2E-quit-confirm: with a running turn, quitting asks; no keeps it, yes ends the host", async ({
  cupper,
}) => {
  await cupper.launch();
  const host = tree((await cupper.connected()).pid as number);
  await runningTurn(cupper);
  const locale = await cupper.app.evaluate(({ app }) => app.getPreferredSystemLanguages()[0]);
  const { t } = createI18n(locale);

  await quit(cupper);
  const asked = await cupper.dialogs.next(0);
  expect(asked).toMatchObject({
    message: t("host:quit_title"),
    detail: t("host:quit_detail", { turns: 1, requests: 0 }),
    buttons: [t("host:quit"), t("host:cancel")],
    defaultId: 1,
    cancelId: 1,
  });
  await cupper.dialogs.answer(asked.index, 1);
  // The app stays, and so does the turn; asked again on the next quit.
  await quit(cupper);
  const again = await cupper.dialogs.next(1);
  expect((await cupper.cli.ok(["status"])).running_turns).toBe(1);

  const closed = cupper.app.waitForEvent("close");
  await cupper.dialogs.answer(again.index, 0);
  await closed;
  // `host/exit` with `force`: the host ends the turn and exits.
  await expect.poll(() => alive(host).length, { timeout: 15_000 }).toBe(0);
});

test("E2E-quit-confirm: idle, quitting asks nothing and leaves the host to its idle rule", async ({
  cupper,
}) => {
  await cupper.launch();
  const { pid } = await cupper.connected();
  const closed = cupper.app.waitForEvent("close");
  // A dialog would keep the app open: the stand-in answers only when told.
  await quit(cupper);
  await closed;
  // No `host/exit`: the host stays, without the app's connections.
  const status = await cupper.cli.ok(["status"]);
  expect(status.pid).toBe(pid);
  expect(
    (status.clients as { principal_kind: string }[]).map((c) => c.principal_kind),
  ).not.toContain("desktop_ui");
});

test("E2E-quit-confirm: on shutdown the app quits without asking, even with a running turn", async ({
  cupper,
}) => {
  await cupper.launch();
  const { pid } = await cupper.connected();
  await runningTurn(cupper);
  await cupper.quitWithoutAsking();
  // The system's SIGTERM is the host's to handle (design 5.7.1); the app sent nothing.
  const status = await cupper.cli.ok(["status"]);
  expect(status).toMatchObject({ pid, running_turns: 1 });
});
