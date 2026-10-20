// The host of this app (design 5.7.2, 5.10.1): the extension in
// `Resources/native/`, `CupperHost.app` in `Helpers/`, the host's environment
// and, in e2e builds, the startup payload the test fixture leaves under
// `CUPPER_E2E_DIR` (design 5.9.9).

import { readFileSync, realpathSync } from "node:fs";
import { dirname, join } from "node:path";

import type { Feature } from "@cupper/protocol";

import type { Flavor } from "../../shared/flavor";
import type { DataConnection } from "../bridge/window-bridge";
import type { MainLog } from "../log";
import { ControlConnection } from "./control";
import { HostLauncher } from "./launcher";
import { loadNative } from "./native";
import { stopLockedHost } from "./restart";

/** What a window's connection asks for (design 5.3.2). */
const WINDOW_FEATURES: Feature[] = ["stream.coalesce", "blob.get", "blob.put", "history.page"];
const TIMEOUT_MS = 5000;

/**
 * The host gets the fixed allowlist of design 5.6.6 from the app's
 * environment: `env.allow` of the runtime manifest, built in.
 */
export function hostEnv(env: NodeJS.ProcessEnv): Record<string, string> {
  const allowed = (name: string) =>
    __CUPPER_HOST_ENV_ALLOW__.some((entry) =>
      entry.endsWith("*") ? name.startsWith(entry.slice(0, -1)) : name === entry,
    );
  return Object.fromEntries(
    Object.entries(env).filter(
      (entry): entry is [string, string] => allowed(entry[0]) && entry[1] !== undefined,
    ),
  );
}

/** e2e: `harness_key` and `test_config.json`, which the fixture writes (design 5.9.9). */
export function e2ePayload(root: string): Record<string, unknown> {
  return {
    harness_key: readFileSync(join(root, "harness_key"), "utf8").trim(),
    test_config: JSON.parse(readFileSync(join(root, "test_config.json"), "utf8")) as unknown,
  };
}

export interface HostSetup {
  flavor: Flavor;
  dataDir: string;
  version: string;
  /** e2e builds only. */
  e2eRoot?: string;
  log: MainLog;
}

/** What restarting the host asks and tells the user. */
export interface RestartDialogs {
  confirm(): Promise<boolean>;
  refused(): Promise<void>;
}

/**
 * Loads the extension and starts the control connection, the host with it if
 * needed; `openWindow` makes the windows' data connections with the same
 * launcher (design 5.7.1). `restart` stops the host in `host.lock` if it is
 * this build's, then connects again (design 5.8.1).
 */
export function startHost({ flavor, dataDir, version, e2eRoot, log }: HostSetup) {
  const contents = join(dirname(process.execPath), "..");
  const native = loadNative(join(process.resourcesPath, "native", "cupper_native.node"), flavor);
  const program = join(contents, "Helpers", "CupperHost.app", "Contents", "MacOS", "cupper-host");
  const launcher = new HostLauncher({
    native,
    program,
    dataDir,
    env: hostEnv(process.env),
    ...(e2eRoot === undefined ? {} : { payload: () => e2ePayload(e2eRoot) }),
  });
  const options = {
    role: "control" as const,
    features: [],
    flavor,
    version,
    timeoutMs: TIMEOUT_MS,
  };
  const control = new ControlConnection(launcher, options, log);
  control.client.onState((state) => log.info("host connection", { ...state }));
  void control.connect();
  const openWindow = async (): Promise<DataConnection> => {
    const window = { role: "window" as const, features: WINDOW_FEATURES, flavor, version };
    const { port, handshake, rest, socket } = await launcher.open({
      ...window,
      timeoutMs: TIMEOUT_MS,
    });
    const onClose = (cb: () => void) => void socket.once("close", cb);
    return {
      port,
      handshake,
      rest,
      pause: () => socket.pause(),
      resume: () => socket.resume(),
      onClose,
    };
  };
  const restart = async (dialogs: RestartDialogs) => {
    if (!(await dialogs.confirm())) {
      return;
    }
    const { native: ext } = native;
    const stopped = stopLockedHost({
      lockFile: join(dataDir, "host.lock"),
      expected: { path: realpathSync(program), cdhash: ext.fileCdhash(program) },
      image: (pid) => ext.processImage(pid),
      terminate: (pid) => process.kill(pid, "SIGTERM"),
    });
    log.info("restarting the host", { outcome: stopped.outcome });
    if (stopped.outcome === "not_this_build") {
      await dialogs.refused();
      return;
    }
    // An orderly exit closes the adapters first (design 5.2.13): up to 10 s.
    for (let waited = 0; stopped.outcome === "stopped" && waited < 10_000; waited += 50) {
      if (ext.processImage(stopped.pid) === null) {
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    await control.connect();
  };
  return { control, openWindow, restart };
}
