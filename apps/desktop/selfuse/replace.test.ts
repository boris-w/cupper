// E2E-selfuse-replace (T160): Cupper Local replaced by hand while it runs.
// At its next connection the old host finds that the code on its path is no
// longer its own, waits to be upgraded and exits once nothing runs (design
// 5.5.7); the new app then runs the new host. The next build, which the job
// built with a host of the dev profile, stands in for a newer one.

import { execFileSync } from "node:child_process";
import { mkdtempSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { App, built, cdhash, cli, HOST_APP, linesSince, logs } from "./app";
import { processes, quitAll, runtimeInstalled, size, until } from "./app";

describe("E2E-selfuse-replace", () => {
  it("the old host waits to be upgraded and exits; the new app runs the new host", async () => {
    const [first, next] = [built("CUPPER_SELFUSE_APP"), built("CUPPER_SELFUSE_NEXT_APP")];
    const [oldHost, newHost] = [first, next].map((app) => cdhash(join(app, HOST_APP)));
    expect(newHost).not.toBe(oldHost);
    const dir = realpathSync(mkdtempSync(join(tmpdir(), "selfuse-")));
    const installed = join(dir, "Cupper Local.app");
    execFileSync("ditto", [first, installed]);
    const old = new App(installed, "cupper-local");
    try {
      await old.ready();
      const running = old.host();
      expect(running.cdhash).toBe(oldHost);
      const hostLog = join(logs("cupper-local"), "host.log");
      const from = size(hostLog);
      // Not while the host copies its runtime from the bundle: it would find
      // the change itself, and go before the new CLI comes.
      await until("the runtime installed", () => runtimeInstalled("cupper-local"), 180_000);

      // As Finder replaces an app: the old bundle goes, the new one comes.
      rmSync(installed, { recursive: true });
      execFileSync("ditto", [next, installed]);
      // The next connection is the new CLI's, which refuses a host of
      // another cdhash (design 5.8.1): exit 4.
      expect(cli(installed, "status").status).toBe(4);
      await until("the old host's exit for the upgrade", () => {
        // host.log's lines: `{message, fields}`.
        const lines = linesSince(hostLog, from).map((line): Record<string, unknown> => ({
          ...(line.fields as object),
          message: line.message,
        }));
        const pending = lines.some((line) => line.event === "host.pending_upgrade");
        const exit = lines.some(
          (line) => line.message === "orderly exit" && line.reason === "upgrade",
        );
        return (pending && exit) || undefined;
      });
      await until("the old host gone", () => !processes().has(running.pid) || undefined);

      // Its host gone, the old UI connects again and starts the host now on
      // disk. Quitting while it connects hangs the app (T225, whose
      // E2E-quit-during-reconnect covers it): first the outcome of both of
      // its connections, as main.log records them.
      await until("the old UI's reconnection", () => {
        const lines = old.log();
        const since = lines.findLastIndex((line) => line.status === "reconnecting");
        const after = since < 0 ? [] : lines.slice(since + 1);
        const control = after.some((line) => ["ready", "failed"].includes(String(line.status)));
        const window = after.some((line) => String(line.msg).startsWith("window connect"));
        return (control && window) || undefined;
      });
      await old.quit();
      const now = new App(installed, "cupper-local");
      try {
        await now.ready();
        expect(now.host().cdhash).toBe(newHost);
        expect(cli(installed, "status").status).toBe(0);
      } finally {
        await quitAll(now);
      }
    } finally {
      await quitAll(old);
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
