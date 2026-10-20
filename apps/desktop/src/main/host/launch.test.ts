// Starting the real e2e host: the `dev-auth` extension and the
// `CupperHost.app` of `cargo xtask host-bundle --kind e2e`, from
// `pnpm test:launch`, which builds both and sets CUPPER_HOST_BUNDLE. Plain
// `pnpm test` skips it. The test signals only the hosts it started.

import { spawnSync } from "node:child_process";
import { once } from "node:events";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { HostClient } from "@cupper/host-client";
import { afterEach, describe, expect, test } from "vitest";

import { e2ePayload, hostEnv } from ".";
import { HostLauncher } from "./launcher";
import { loadNative } from "./native";

const bundle = process.env.CUPPER_HOST_BUNDLE;
const TARGET = resolve(import.meta.dirname, "../../../../../target");

const started: number[] = [];
const dirs: string[] = [];
afterEach(async () => {
  for (const pid of started.splice(0).filter(alive)) {
    process.kill(pid, "SIGTERM");
    while (alive(pid)) {
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
  }
  dirs.splice(0).forEach((dir) => rmSync(dir, { recursive: true, force: true }));
});

function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

describe.skipIf(bundle === undefined)("the e2e host bundle", () => {
  test("starts, handshakes, answers, and starts again after it is killed", async () => {
    const root = mkdtempSync(join(tmpdir(), "cupper-launch-"));
    dirs.push(root);
    writeFileSync(join(root, "harness_key"), `${"ab".repeat(32)}\n`, { mode: 0o600 });
    writeFileSync(join(root, "test_config.json"), "{}");
    // The first run of a new binary waits for the system's check (about 2 s
    // here), longer than the 500 ms budget: run it once first.
    const program = resolve(bundle ?? "", "CupperHost.app/Contents/MacOS/cupper-host");
    spawnSync(program, ["--build-info"]);
    const launcher = new HostLauncher({
      native: loadNative(join(TARGET, "debug/libcupper_native.dylib"), "e2e"),
      program,
      dataDir: join(root, "data"),
      env: hostEnv(process.env),
      payload: () => e2ePayload(root),
    });
    const options = {
      role: "control",
      features: [],
      flavor: "e2e",
      version: "0",
      timeoutMs: 5000,
    } as const;
    const lockedPid = () => Number(readFileSync(join(root, "data/host.lock"), "utf8"));

    const first = await launcher.open({ ...options, features: [] });
    const pid = lockedPid();
    started.push(pid);
    expect(first.conn.hostPid).toBe(pid);
    expect(first.handshake.host.weak_auth).toBe(true);
    const client = new HostClient();
    client.attach(first.port, first.handshake, first.rest);
    expect(await client.request("ping")).toEqual({});

    process.kill(pid, "SIGKILL");
    await once(first.socket, "close");
    const second = await launcher.open({ ...options, features: [] });
    started.push(lockedPid());
    expect(lockedPid()).not.toBe(pid);
    expect(second.conn.hostPid).toBe(lockedPid());
    second.port.close();
    client.close();
  });
});
