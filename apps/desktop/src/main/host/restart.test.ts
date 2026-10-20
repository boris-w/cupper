// UNIT-host-restart: the signal goes only to this build's host; quitting asks
// only when something would be lost, and never on shutdown or an update.

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import type { HostStatusResult } from "@cupper/protocol";
import { afterEach, describe, expect, test } from "vitest";

import { QuitGuard, type QuitHost } from "./lifecycle";
import { stopLockedHost } from "./restart";

const dirs: string[] = [];
afterEach(() => dirs.splice(0).forEach((dir) => rmSync(dir, { recursive: true, force: true })));

const HOST = {
  path: "/App/Contents/Helpers/CupperHost.app/Contents/MacOS/cupper-host",
  cdhash: "a".repeat(40),
};

function stop(running: { path: string; cdhash: string } | null, lock = "4321\n") {
  const dir = mkdtempSync(join(tmpdir(), "cupper-restart-"));
  dirs.push(dir);
  writeFileSync(join(dir, "host.lock"), lock);
  const signalled: number[] = [];
  const outcome = stopLockedHost({
    lockFile: join(dir, "host.lock"),
    expected: HOST,
    image: (pid) => (pid === 4321 ? running : null),
    terminate: (pid) => signalled.push(pid),
  });
  return { outcome, signalled };
}

function quitting(status: Partial<HostStatusResult> | undefined, answer = true) {
  const calls = { confirms: 0, exits: [] as boolean[] };
  const host: QuitHost = {
    status: () =>
      status === undefined
        ? Promise.reject(new Error("not connected"))
        : Promise.resolve({
            running_turns: 0,
            pending_interactions: 0,
            ...status,
          } as HostStatusResult),
    exit: (force) => Promise.resolve(void calls.exits.push(force)),
  };
  const guard = new QuitGuard(
    () => host,
    () => Promise.resolve(void (calls.confirms += 1)).then(() => answer),
  );
  return { guard, calls };
}

describe("UNIT-host-restart", () => {
  test("only this build's host, same path and cdhash, gets SIGTERM", () => {
    expect(stop(HOST)).toEqual({ outcome: { outcome: "stopped", pid: 4321 }, signalled: [4321] });
    const elsewhere = stop({ ...HOST, path: "/tmp/cupper-host" });
    expect(elsewhere).toEqual({ outcome: { outcome: "not_this_build", pid: 4321 }, signalled: [] });
    const rebuilt = stop({ ...HOST, cdhash: "b".repeat(40) });
    expect(rebuilt).toEqual({ outcome: { outcome: "not_this_build", pid: 4321 }, signalled: [] });
    for (const lock of ["", "1\n", "not a pid", "999\n"]) {
      expect(stop(HOST, lock), lock).toEqual({ outcome: { outcome: "no_host" }, signalled: [] });
    }
  });

  test("quitting asks only while turns run or requests wait", async () => {
    for (const [status, asked] of [
      [{ running_turns: 1 }, true],
      [{ pending_interactions: 2 }, true],
      [{}, false],
      [undefined, false],
    ] as const) {
      const { guard, calls } = quitting(status);
      expect(guard.settled).toBe(false);
      expect(await guard.decide()).toBe("quit");
      expect(guard.settled).toBe(true);
      expect(calls).toEqual({ confirms: asked ? 1 : 0, exits: asked ? [true] : [] });
    }
  });

  test("a cancelled confirmation keeps the app; asking twice shows one dialog", async () => {
    const { guard, calls } = quitting({ running_turns: 1 }, false);
    const [one, two] = await Promise.all([guard.decide(), guard.decide()]);
    expect([one, two]).toEqual(["stay", "stay"]);
    expect(calls).toEqual({ confirms: 1, exits: [] });
    expect(guard.settled).toBe(false);
  });

  test("shutdown and installing an update quit without asking", () => {
    const { guard, calls } = quitting({ running_turns: 3 });
    guard.skip();
    expect(guard.settled).toBe(true);
    expect(calls).toEqual({ confirms: 0, exits: [] });
  });
});
