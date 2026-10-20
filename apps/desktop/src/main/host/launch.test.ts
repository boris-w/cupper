// Starting the real e2e host: the `dev-auth` extension and the
// `CupperHost.app` of `cargo xtask host-bundle --kind e2e`, from
// `pnpm test:launch`, which builds both and sets CUPPER_HOST_BUNDLE. Plain
// `pnpm test` skips it. The test signals only the hosts it started.

import { once } from "node:events";
import { mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { HostClient } from "@cupper/host-client";
import { afterEach, describe, expect, test } from "vitest";

import { e2ePayload, hostEnv } from ".";
import { HostFailure } from "./connection";
import { HostLauncher } from "./launcher";
import { loadNative } from "./native";
import { stopLockedHost } from "./restart";

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

const OPTIONS = { role: "control", flavor: "e2e", version: "0", timeoutMs: 5000 } as const;

/** A data directory with the fixture's files, and launchers on it, each with its own key. */
function setup() {
  const root = mkdtempSync(join(tmpdir(), "cupper-launch-"));
  dirs.push(root);
  writeFileSync(join(root, "harness_key"), `${"ab".repeat(32)}\n`, { mode: 0o600 });
  writeFileSync(join(root, "test_config.json"), "{}");
  const native = loadNative(join(TARGET, "debug/libcupper_native.dylib"), "e2e");
  const program = resolve(bundle ?? "", "CupperHost.app/Contents/MacOS/cupper-host");
  const launcher = () =>
    new HostLauncher({
      native,
      program,
      dataDir: join(root, "data"),
      env: hostEnv(process.env),
      payload: () => e2ePayload(root),
    });
  const lockedPid = () => Number(readFileSync(join(root, "data/host.lock"), "utf8"));
  return { root, native, program, launcher, lockedPid };
}

describe.skipIf(bundle === undefined)("the e2e host bundle", () => {
  test("starts, handshakes, answers, and starts again after it is killed", async () => {
    const { launcher: make, lockedPid } = setup();
    const launcher = make();
    // A host bundle just built runs for the first time here: the launcher
    // waits for the system's check (design 5.7.2).
    const first = await launcher.open({ ...OPTIONS, features: [] });
    const pid = lockedPid();
    started.push(pid);
    expect(first.conn.hostPid).toBe(pid);
    expect(first.handshake.host.weak_auth).toBe(true);
    const client = new HostClient();
    client.attach(first.port, first.handshake, first.rest);
    expect(await client.request("ping")).toEqual({});

    process.kill(pid, "SIGKILL");
    await once(first.socket, "close");
    const second = await launcher.open({ ...OPTIONS, features: [] });
    started.push(lockedPid());
    expect(lockedPid()).not.toBe(pid);
    expect(second.conn.hostPid).toBe(lockedPid());
    second.port.close();
    client.close();
  });

  test("a host another key started is unverified; stopped as this build's, it starts again", async () => {
    const { root, native, program, launcher, lockedPid } = setup();
    const first = await launcher().open({ ...OPTIONS, features: [] });
    const pid = lockedPid();
    started.push(pid);
    first.port.close();

    const again = launcher();
    const refused = await again.open({ ...OPTIONS, features: [] }).catch((e: unknown) => e);
    expect(refused).toEqual(new HostFailure("unauthenticated"));
    const stopped = stopLockedHost({
      lockFile: join(root, "data/host.lock"),
      expected: { path: realpathSync(program), cdhash: native.native.fileCdhash(program) },
      image: (pid) => native.native.processImage(pid),
      terminate: (pid) => process.kill(pid, "SIGTERM"),
    });
    expect(stopped).toEqual({ outcome: "stopped", pid });
    while (native.native.processImage(pid) !== null) {
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    const second = await again.open({ ...OPTIONS, features: [] });
    started.push(lockedPid());
    expect(lockedPid()).not.toBe(pid);
    second.port.close();
  });
});
