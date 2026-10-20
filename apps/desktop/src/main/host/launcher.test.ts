// UNIT-launcher, starting the host: the native extension, the socket and the
// handshake are fakes; time is virtual. The real thing is in launch.it.ts.

import { readFileSync } from "node:fs";

import type {
  DevAuthNative,
  HostConnection,
  HostProcess,
  KeyedHostConnection,
  ReleaseNative,
} from "@cupper/desktop-native";
import { HandshakeError } from "@cupper/host-client";
import { MockHost, type MethodFixture } from "@cupper/host-client/mock";
import { describe, expect, test } from "vitest";

import { handshakeHost, HostFailure, type Connected, type ConnectOptions } from "./connection";
import { HostLauncher } from "./launcher";
import { checkNative } from "./native";

const OPTIONS: ConnectOptions = {
  role: "control",
  features: [],
  flavor: "e2e",
  version: "0.0.0",
  timeoutMs: 1000,
};

/** A host that is not running until `listening`, and exits as `exits` say. */
function fakeHost(keyed: boolean) {
  const state = {
    listening: false,
    time: 0,
    spawns: [] as { args: string[]; key: unknown; payload: unknown }[],
    dials: [] as unknown[],
    exits: [] as Promise<{ code?: number }>[],
    payloads: 0,
    /** What happens when virtual time reaches a point. */
    at: [] as [number, () => void][],
  };
  class HostKey {
    readonly fake = true;
  }
  const spawnHost = (command: { args: string[] }, key?: unknown, payload?: unknown) => {
    state.spawns.push({ args: command.args, key, payload });
    return { pid: 1, exit: state.exits.shift() ?? new Promise(() => undefined) } as HostProcess;
  };
  const connectHost = (_socket: string, key?: unknown) => {
    state.dials.push(key);
    if (!state.listening) {
      throw Object.assign(new Error("nothing listens"), { code: "connect_failed" });
    }
    return {} as HostConnection;
  };
  const base = { socketPath: (dir: string) => `${dir}/run/host.sock`, spawnHost, connectHost };
  const native = keyed
    ? ({
        ...base,
        buildInfo: () => ({ features: ["dev-auth"], hostCheck: "key_proof" }),
        HostKey,
      } as unknown as DevAuthNative)
    : ({
        ...base,
        buildInfo: () => ({ features: [], hostCheck: "signature" }),
      } as unknown as ReleaseNative);
  const launcher = new HostLauncher({
    native: checkNative(native, keyed ? "e2e" : "release"),
    program: "/App/Contents/Helpers/CupperHost.app/Contents/MacOS/cupper-host",
    dataDir: "/tmp/case/data",
    env: {},
    payload: () => {
      state.payloads += 1;
      return { harness_key: "a".repeat(64), test_config: {} };
    },
    clock: {
      now: () => state.time,
      sleep: (ms) => {
        state.time += ms;
        state.at.filter(([time]) => time <= state.time).forEach(([, event]) => event());
        state.at = state.at.filter(([time]) => time > state.time);
        return Promise.resolve();
      },
    },
    open: (conn) => Promise.resolve({ conn } as Connected),
  });
  return { state, launcher };
}

const failure = (promise: Promise<unknown>) =>
  promise.then(
    () => "connected",
    (error: unknown) => (error instanceof HostFailure ? error.reason : String(error)),
  );

describe("UNIT-launcher", () => {
  test("the payload stays in memory and a host started again gets it and the key again", async () => {
    const { state, launcher } = fakeHost(true);
    state.exits.push(new Promise(() => undefined));
    const first = launcher.open(OPTIONS);
    await Promise.resolve();
    state.listening = true;
    await first;
    // The host dies; the next connection starts it again.
    state.listening = false;
    const second = launcher.open(OPTIONS);
    await Promise.resolve();
    state.listening = true;
    await second;
    expect(state.spawns).toHaveLength(2);
    const [one, two] = state.spawns;
    expect(one?.args).toEqual(["--data-dir", "/tmp/case/data"]);
    expect(two?.payload).toBe(one?.payload);
    expect(two?.key).toBe(one?.key);
    expect(state.payloads).toBe(1);
    expect(new Set(state.dials)).toEqual(new Set([one?.key]));
  });

  test("a release build starts the host without a payload", async () => {
    const { state, launcher } = fakeHost(false);
    state.listening = true;
    await launcher.open({ ...OPTIONS, flavor: "release" });
    expect(state.spawns).toEqual([]);
    state.listening = false;
    expect(await failure(launcher.open({ ...OPTIONS, flavor: "release" }))).toBe(
      "host_unreachable",
    );
    expect(state.spawns).toEqual([
      { args: ["--data-dir", "/tmp/case/data"], key: undefined, payload: undefined },
    ]);
    expect(state.payloads).toBe(0);
  });

  test("connecting is tried every 10 ms, up to 10 s while the host we started runs", async () => {
    const { state, launcher } = fakeHost(true);
    expect(await failure(launcher.open(OPTIONS))).toBe("host_unreachable");
    expect(state.time).toBe(10_000);
    expect(state.dials).toHaveLength(1 + 1000);
    launcher.installing = true;
    expect(await failure(launcher.open(OPTIONS))).toBe("updating");
    expect(state.spawns).toHaveLength(1);
  });

  test("a host slow to start is waited for; one that exits meanwhile gives its reason", async () => {
    const slow = fakeHost(true);
    slow.state.at.push([2000, () => (slow.state.listening = true)]);
    expect(await failure(slow.launcher.open(OPTIONS))).toBe("connected");
    expect(slow.state.time).toBe(2000);

    const failing = fakeHost(true);
    let exit: (value: { code?: number }) => void = () => undefined;
    failing.state.exits.push(new Promise((resolve) => (exit = resolve)));
    failing.state.at.push([3000, () => exit({ code: 5 })]);
    expect(await failure(failing.launcher.open(OPTIONS))).toBe("data_dir_unusable");
    expect(failing.state.time).toBe(3000);

    // Another host holds the lock: it should be listening, so 500 ms are enough.
    const other = fakeHost(true);
    other.state.exits.push(Promise.resolve({ code: 3 }));
    expect(await failure(other.launcher.open(OPTIONS))).toBe("host_unreachable");
    expect(other.state.time).toBe(500);
  });

  test("exit codes 4 to 6 give their reasons; with 3 another host is connected to", async () => {
    for (const [code, reason] of [
      [4, "data_too_new"],
      [5, "data_dir_unusable"],
      [6, "migration_failed"],
      [1, "host_exited"],
    ] as const) {
      const { state, launcher } = fakeHost(true);
      state.exits.push(Promise.resolve({ code }));
      expect(await failure(launcher.open(OPTIONS))).toBe(reason);
      expect(state.time).toBe(10);
    }
    const { state, launcher } = fakeHost(true);
    state.exits.push(Promise.resolve({ code: 3 }));
    const opening = failure(launcher.open(OPTIONS));
    for (let turn = 0; turn < 10; turn++) {
      await Promise.resolve();
    }
    state.listening = true;
    expect(await opening).toBe("connected");
  });

  test("only a release build refuses a host in the degraded mode", async () => {
    const fixtures = JSON.parse(
      readFileSync(
        new URL("../../../../../protocol/fixtures/methods.json", import.meta.url),
        "utf8",
      ),
    ) as MethodFixture[];
    const keyed = {
      auth: (nonce: string) => ({ type: "dev_key", c_nonce: nonce, proof: "c".repeat(64) }),
      verifyHostProof: () => true,
    } as unknown as KeyedHostConnection;
    for (const [flavor, conn, refused] of [
      ["release", undefined, true],
      ["local", keyed, false],
      ["dev", keyed, false],
    ] as const) {
      const host = new MockHost(fixtures);
      expect(host.init.host.weak_auth).toBe(true);
      const port = host.connect().port();
      const signal = new AbortController().signal;
      const done = handshakeHost(port, conn, { ...OPTIONS, flavor }, signal);
      if (refused) {
        await expect(done).rejects.toEqual(new HandshakeError("weak_auth"));
      } else {
        expect((await done).handshake.host.weak_auth).toBe(true);
      }
    }
  });

  test("an extension of another build is refused", () => {
    const release = {
      buildInfo: () => ({ features: [], hostCheck: "signature" }),
    } as unknown as ReleaseNative;
    expect(() => checkNative(release, "e2e")).toThrow(/not the e2e build's/);
    expect(checkNative(release, "release").keyed).toBe(false);
  });
});
