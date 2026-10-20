// hostLauncher (design 5.7.2, 5.5.1): a connection to the host, starting the
// host through the native extension when nothing listens. Keyed builds make
// one `HostKey` and keep the startup payload only in memory; a host started
// again after it died gets the same key and the same payload. Connecting is
// tried every 10 ms for 500 ms after a start; the host's exit codes 4–6
// (design 5.2.13) end the attempt with their reasons, and 3 (another host
// holds the lock) means connecting to that one.

import type { DevAuthNative, HostKey, HostProcess, ReleaseNative } from "@cupper/desktop-native";
import type { ConnectionFailure } from "@cupper/shell-api";

import { HostFailure, openConnection, type Connected, type ConnectOptions } from "./connection";
import type { Native } from "./native";

export interface Clock {
  now(): number;
  sleep(ms: number): Promise<void>;
}

const realClock: Clock = {
  now: () => performance.now(),
  sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
};

const RETRY_MS = 10;
const BUDGET_MS = 500;

/** The exit codes that end an attempt, as `failed.reason`. */
const EXIT_REASONS: Partial<Record<number, ConnectionFailure>> = {
  4: "data_too_new",
  5: "data_dir_unusable",
  6: "migration_failed",
};

export interface LaunchConfig {
  native: Native;
  /** `Contents/Helpers/CupperHost.app/Contents/MacOS/cupper-host`. */
  program: string;
  dataDir: string;
  /** The host's whole environment. */
  env: Record<string, string>;
  /** Keyed builds: more of the startup payload, asked for once (e2e: `harness_key`, `test_config`). */
  payload?: () => Record<string, unknown>;
  clock?: Clock;
  /** Tests stand in for the socket and the handshake. */
  open?: typeof openConnection;
}

export class HostLauncher {
  /** While an update installs, the host is not started (design 5.10.5). */
  installing = false;
  readonly #config: LaunchConfig;
  readonly #socket: string;
  /** The extension, with the key in keyed builds. */
  readonly #native:
    { native: DevAuthNative; key: HostKey } | { native: ReleaseNative; key?: never };
  #payload: Record<string, unknown> | undefined;

  constructor(config: LaunchConfig) {
    this.#config = config;
    const { native } = config;
    this.#socket = native.native.socketPath(config.dataDir);
    this.#native = native.keyed
      ? { native: native.native, key: new native.native.HostKey() }
      : { native: native.native };
  }

  /** A handshaken connection; throws {@link HostFailure}. */
  async open(options: ConnectOptions): Promise<Connected> {
    const first = await this.#connect(options);
    if (first !== "absent") {
      return first;
    }
    if (this.installing) {
      throw new HostFailure("updating");
    }
    const clock = this.#config.clock ?? realClock;
    const deadline = clock.now() + BUDGET_MS;
    let exited: Promise<number | undefined> | undefined = this.#spawn().exit.then((e) => e.code);
    for (;;) {
      const tick = clock.sleep(RETRY_MS).then(() => "tick" as const);
      const outcome = await (exited ? Promise.race([tick, exited]) : tick);
      if (outcome !== "tick") {
        if (outcome !== 3) {
          throw new HostFailure((outcome !== undefined && EXIT_REASONS[outcome]) || "host_exited");
        }
        exited = undefined;
      }
      const next = await this.#connect(options);
      if (next !== "absent") {
        return next;
      }
      if (clock.now() >= deadline) {
        throw new HostFailure("host_unreachable");
      }
    }
  }

  async #connect(options: ConnectOptions): Promise<Connected | "absent"> {
    const it = this.#native;
    let conn;
    try {
      conn = it.key
        ? it.native.connectHost(this.#socket, it.key)
        : it.native.connectHost(this.#socket);
    } catch (error) {
      // `connect_failed`: nothing listens. The rest are refusals (T073).
      const code = (error as { code?: unknown } | null)?.code;
      if (code === "connect_failed") {
        return "absent";
      }
      throw new HostFailure(typeof code === "string" ? code : "unexpected", { cause: error });
    }
    return (this.#config.open ?? openConnection)(conn, it.key !== undefined, options);
  }

  #spawn(): HostProcess {
    const { program, dataDir, env } = this.#config;
    const command = { program, args: ["--data-dir", dataDir], env };
    const it = this.#native;
    try {
      if (!it.key) {
        return it.native.spawnHost(command);
      }
      this.#payload ??= this.#config.payload?.() ?? {};
      return it.native.spawnHost(command, it.key, this.#payload);
    } catch (error) {
      const code = (error as { code?: unknown } | null)?.code;
      throw new HostFailure(typeof code === "string" ? code : "spawn_failed", { cause: error });
    }
  }
}
