// The host of this app (design 5.7.2, 5.10.1): the extension in
// `Resources/native/`, `CupperHost.app` in `Helpers/`, the host's environment
// and, in e2e builds, the startup payload the test fixture leaves under
// `CUPPER_E2E_DIR` (design 5.9.9).

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";

import type { Flavor } from "../../shared/flavor";
import type { MainLog } from "../log";
import { ControlConnection } from "./control";
import { HostLauncher } from "./launcher";
import { loadNative } from "./native";

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

/** Loads the extension and starts the control connection, the host with it if needed. */
export function startControl({ flavor, dataDir, version, e2eRoot, log }: HostSetup) {
  const contents = join(dirname(process.execPath), "..");
  const native = loadNative(join(process.resourcesPath, "native", "cupper_native.node"), flavor);
  const launcher = new HostLauncher({
    native,
    program: join(contents, "Helpers", "CupperHost.app", "Contents", "MacOS", "cupper-host"),
    dataDir,
    env: hostEnv(process.env),
    ...(e2eRoot === undefined ? {} : { payload: () => e2ePayload(e2eRoot) }),
  });
  const options = { role: "control" as const, features: [], flavor, version, timeoutMs: 5000 };
  const control = new ControlConnection(launcher, options, log);
  control.client.onState((state) => log.info("host connection", { ...state }));
  void control.connect();
  return control;
}
