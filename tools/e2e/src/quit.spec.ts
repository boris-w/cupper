// E2E-quit-during-reconnect (T225): ⌘Q quits whatever the control connection
// is doing. With no host to ask, the quit's decision used to come within
// Electron's `before-quit`, which undid the `app.quit()` made there: the
// window closed and the app stayed. The quit comes as SIGTERM, from outside
// JavaScript, as the menu's does; one made from JavaScript would defer the
// decision itself. Changing the bundle under the running app takes a copy of
// the app of the test's own.

import { execFileSync } from "node:child_process";
import { readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Cupper } from "./fixture";
import { killHost } from "./helpers";
import { BUILT, IN_APP } from "./paths";
import { gone, processes, signal } from "./procs";

/** A copy of the app in the test's root. */
function copy(cupper: Cupper, name: string): string {
  const bundle = join(cupper.root, name, "Cupper E2E.app");
  execFileSync("ditto", [BUILT.bundle, bundle]);
  return bundle;
}

/** A next version in `bundle`'s place, as a user replaces the app: its host of another cdhash. */
function replace(cupper: Cupper, bundle: string) {
  const next = copy(cupper, "next");
  const host = join(next, "Contents/Helpers/CupperHost.app");
  execFileSync("codesign", ["--force", "--sign", "-", "--identifier", "next.host", host]);
  rmSync(bundle, { recursive: true });
  execFileSync("ditto", [next, bundle]);
}

const log = (cupper: Cupper) =>
  readFileSync(join(cupper.root, "logs/main.log"), "utf8")
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line) as { msg: string; status?: string });

const states = (cupper: Cupper) =>
  log(cupper).flatMap((line) => (line.msg === "host connection" ? [line.status] : []));

/**
 * SIGTERM: the app is gone within 10 s, through `will-quit`; it takes no
 * connection once it has decided to quit, and no host of the test stays.
 */
async function quits(cupper: Cupper) {
  const app = cupper.app.process();
  const exited = new Promise<boolean>((done) => {
    app.once("exit", () => done(true));
    setTimeout(() => done(false), 10_000);
  });
  app.kill("SIGTERM");
  expect(await exited, "the app quit within 10 s").toBe(true);
  const lines = log(cupper);
  const decided = lines.map((line) => line.msg).lastIndexOf("before-quit");
  const quit = lines.flatMap((line) => (/quit$/.test(line.msg) ? [line.msg] : []));
  expect(quit).toEqual(["before-quit", "before-quit", "will-quit", "quit"]);
  expect(lines.slice(decided).filter((line) => line.status === "ready")).toEqual([]);
  const hosts = () => processes().filter((p) => p.args.endsWith(`--data-dir ${cupper.dataDir}`));
  await expect.poll(hosts, { timeout: 10_000 }).toEqual([]);
}

test.describe("E2E-quit-during-reconnect", () => {
  test("(a) while the control connection connects again", async ({ cupper }) => {
    const bundle = copy(cupper, "app");
    await cupper.launch({ bundle });
    await cupper.connected();
    // The next host takes 5 s to start: the app connects again all that time.
    const host = join(bundle, IN_APP.host);
    renameSync(host, `${host}.real`);
    writeFileSync(host, '#!/bin/sh\nsleep 5\nexec "$0.real" "$@"\n', { mode: 0o755 });
    signal(
      [cupper.host()].filter((proc) => proc !== undefined),
      "SIGKILL",
    );
    await expect.poll(() => states(cupper)).toContain("reconnecting");
    await quits(cupper);
  });

  test("(a) once connecting again has failed", async ({ cupper }) => {
    const bundle = copy(cupper, "app");
    await cupper.launch({ bundle });
    await cupper.connected();
    rmSync(join(bundle, IN_APP.host));
    signal(
      [cupper.host()].filter((proc) => proc !== undefined),
      "SIGKILL",
    );
    await expect.poll(() => states(cupper)).toContain("failed");
    await quits(cupper);
  });

  test("(b) the host killed, the bundle as it was", async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    await killHost(cupper);
    await quits(cupper);
  });

  test("(c) the bundle replaced, its host still running", async ({ cupper }) => {
    const bundle = copy(cupper, "app");
    await cupper.launch({ bundle });
    await cupper.connected();
    replace(cupper, bundle);
    await quits(cupper);
  });

  test("(d) the bundle replaced, its host gone for the upgrade", async ({ cupper }) => {
    const bundle = copy(cupper, "app");
    await cupper.launch({ bundle });
    await cupper.connected();
    const host = cupper.host();
    replace(cupper, bundle);
    // The next connection finds the code on its path changed (design 5.5.7).
    await cupper.cli.run(["status"]);
    expect(
      await gone(
        [host].filter((proc) => proc !== undefined),
        15_000,
      ),
    ).toEqual([]);
    await quits(cupper);
  });
});
