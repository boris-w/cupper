// Restarting the host (design 5.8.1, degraded mode): a new UI instance cannot
// prove the key of a host an earlier instance started (`unauthenticated`,
// `host_unverified`), so the user is offered to restart it. The pid in
// `host.lock` gets SIGTERM only when it runs this build's host, the same
// executable with the same cdhash; anything else there is left alone.

import { readFileSync } from "node:fs";

import type { ProcessImage } from "@cupper/desktop-native";

export interface StopHost {
  /** `<data dir>/host.lock`. */
  lockFile: string;
  /** This build's `cupper-host`: its real path and cdhash. */
  expected: ProcessImage;
  image(pid: number): ProcessImage | null;
  terminate(pid: number): void;
}

export type StopOutcome =
  | { outcome: "stopped"; pid: number }
  | { outcome: "no_host" }
  | { outcome: "not_this_build"; pid: number };

/** Sends SIGTERM to the host in `host.lock`, if it is this build's. */
export function stopLockedHost(host: StopHost): StopOutcome {
  let pid = Number.NaN;
  try {
    pid = Number(readFileSync(host.lockFile, "utf8").trim());
  } catch {
    // No lock file: no host ever ran here.
  }
  const image = Number.isSafeInteger(pid) && pid > 1 ? host.image(pid) : null;
  if (image === null) {
    return { outcome: "no_host" };
  }
  if (image.path !== host.expected.path || image.cdhash !== host.expected.cdhash) {
    return { outcome: "not_this_build", pid };
  }
  host.terminate(pid);
  return { outcome: "stopped", pid };
}
