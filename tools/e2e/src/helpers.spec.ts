// The helpers' own checks: each helper does what it says on the real app.
// They carry no test ID and run in the nightly set (T167); those that need
// the window's focus, for real input, are tagged `@focus`.

import { expect, test } from "./fixture";
import {
  advanceClock,
  disconnectWindow,
  elevate,
  events,
  hostClient,
  killHost,
  sampleNetwork,
  texts,
  untilEvent,
  untilSession,
} from "./helpers";
import { activatePanel, focusWindow, panelReady, pressDigit, requestPanel, type } from "./input";

test("helpers: __cupperTest and dropping the window's connection", async ({ cupper }) => {
  await cupper.launch();
  await cupper.connected();
  const client = hostClient(cupper);
  await expect.poll(client.state).toEqual({ status: "ready" });
  await disconnectWindow(cupper);
  await expect.poll(client.state).toEqual({ status: "ready" });
  expect(await client.counters()).toEqual({ gaps: 0, duplicates: 0, resets: 0 });
});

test.describe("on the manual clock", () => {
  test.use({ testConfig: { clock: "manual" } });

  test("helpers: the clock, the index and the session's events", async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    const { session_id } = await cupper.cli.ok([
      "new",
      "--agent",
      "claude",
      "--project",
      cupper.repo,
      "say hi",
    ]);
    const stream = `session/${String(session_id)}`;
    await untilEvent(cupper, stream, (event) => event.type === "turn.completed");
    await advanceClock(cupper, 1000);
    expect(await untilSession(cupper, String(session_id))).toMatchObject({ session_id });
    // The host found git on the fixture's PATH.
    const project = events(cupper, "index").find((event) => event.type === "project.upserted");
    expect(project?.data.is_git).toBe(true);
  });
});

test("helpers: kill -9 the host, and the app starts another", async ({ cupper }) => {
  await cupper.launch();
  const { pid } = await cupper.connected();
  const status = await killHost(cupper);
  expect(status.pid).not.toBe(pid);
  await expect.poll(hostClient(cupper).state).toEqual({ status: "ready" });
});

test(
  "helpers: the CLI elevates through the banner and the dialog, then writes",
  { tag: "@focus" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    const repl = await elevate(cupper);
    const created = await repl.run(`new --agent claude --project ${cupper.repo} hi`);
    expect(created.code, created.stderr).toBe(0);
  },
);

test("helpers: a network sample has the app's flows", async ({ cupper }) => {
  await cupper.launch();
  await cupper.connected();
  // A flow of the main process's own, on the loopback.
  const port = await cupper.app.evaluate(async () => {
    const net = process.getBuiltinModule("node:net");
    const server = net.createServer().listen(0, "127.0.0.1");
    await new Promise((listening) => server.once("listening", listening));
    const { port } = server.address() as { port: number };
    Object.assign(globalThis, { __e2eFlow: net.connect(port, "127.0.0.1") });
    return port;
  });
  const main = cupper.app.process().pid;
  await expect
    .poll(() => sampleNetwork(cupper).some((f) => f.pid === main && f.remote.endsWith(`:${port}`)))
    .toBe(true);
});

test(
  "helpers: keys as real input type into the composer",
  { tag: "@focus" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    const composer = cupper.window.locator("[data-composer] textarea");
    await composer.focus();
    await focusWindow(cupper);
    await type(cupper, "hi");
    await expect(composer).toHaveValue("hi");
  },
);

test(
  "helpers: the request panel, armed, tabbed into and answered with a digit",
  { tag: "@focus" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    const { session_id } = await cupper.cli.ok([
      "new",
      "--agent",
      "claude",
      "--project",
      cupper.repo,
      "permission p1",
    ]);
    const stream = `session/${String(session_id)}`;
    const asked = await untilEvent(
      cupper,
      stream,
      (event) => event.type === "interaction.requested",
    );
    await cupper.window.getByRole("button", { name: /permission p1/ }).click();
    await panelReady(cupper);
    await focusWindow(cupper);
    await activatePanel(cupper);
    await pressDigit(cupper, 1);
    await expect(requestPanel(cupper)).toHaveCount(0);
    const resolved = await untilEvent(
      cupper,
      stream,
      (event) => event.type === "interaction.resolved",
    );
    expect(resolved.data.request_id).toBe(asked.data.request_id);
  },
);

test("helpers: the open and the synchronous dialogs", { tag: "@focus" }, async ({ cupper }) => {
  await cupper.launch();
  await cupper.connected();
  const t = await texts(cupper);
  await cupper.window.getByRole("button", { name: t("sidebar:add_project") }).click();
  const open = await cupper.dialogs.next();
  expect(open).toMatchObject({
    kind: "open",
    properties: expect.arrayContaining(["openDirectory"]),
  });
  await cupper.dialogs.choose(open.index, [cupper.repo]);
  await expect(cupper.window.getByRole("region", { name: "repo" })).toBeVisible();

  const response = await cupper.app.evaluate(({ dialog }) =>
    dialog.showMessageBoxSync({ message: "sync", buttons: ["yes", "no"], cancelId: 1 }),
  );
  expect(response).toBe(1);
  expect((await cupper.dialogs.calls()).at(-1)).toMatchObject({ kind: "message_sync" });
});
