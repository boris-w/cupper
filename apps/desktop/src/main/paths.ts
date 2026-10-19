import { isAbsolute, join } from "node:path";

import type { Flavor } from "../shared/flavor";

export interface AppDirs {
  /** The host's data directory (design 5.4.1). */
  data: string;
  /**
   * Electron's userData, set explicitly: by default it would be
   * `Application Support/Cupper`, the same directory as the host's `cupper`
   * on a case-insensitive volume.
   */
  userData: string;
  /** Where `main.log` goes (design 5.11.1). */
  logs: string;
}

/** Each flavor's directory name; flavors never share data (design 5.4.1). */
const DIR_NAMES: Record<Exclude<Flavor, "e2e">, string> = {
  release: "cupper",
  local: "cupper-local",
  dev: "cupper-dev",
};

export function appDirs(
  flavor: Exclude<Flavor, "e2e">,
  env: { appData: string; home: string },
): AppDirs {
  const name = DIR_NAMES[flavor];
  return withUserData(join(env.appData, name), join(env.home, "Library", "Logs", name));
}

/**
 * An e2e build keeps everything under the test case's temporary root
 * (`CUPPER_E2E_DIR`, design 5.9.9), which must be absolute. Only e2e builds
 * call this, so release builds do not contain it.
 */
export function e2eDirs(root: string | undefined): AppDirs {
  if (root === undefined || !isAbsolute(root)) {
    throw new Error("an e2e build needs CUPPER_E2E_DIR, an absolute path");
  }
  return withUserData(join(root, "data"), join(root, "logs"));
}

function withUserData(data: string, logs: string): AppDirs {
  return { data, userData: join(data, "ui"), logs };
}
