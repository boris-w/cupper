// controlClient (design 5.7.2): the main process's own connection to the host,
// a HostClient on it. When the connection drops, it connects again; the
// launcher starts the host again if it is gone. The window bridge (T076) makes
// the window connections; quitting checks (T007) ask `host/status` here.

import { HostClient } from "@cupper/host-client";

import type { MainLog } from "../log";
import { HostFailure, type ConnectOptions } from "./connection";
import type { HostLauncher } from "./launcher";

export class ControlConnection {
  readonly client: HostClient;
  readonly #launcher: HostLauncher;
  readonly #options: ConnectOptions;
  #closed = false;

  constructor(launcher: HostLauncher, options: ConnectOptions, log: MainLog) {
    this.#launcher = launcher;
    this.#options = options;
    this.client = new HostClient({ log: (message) => log.warn("control connection", { message }) });
  }

  /** Connects; again each time the connection drops, until {@link close}. */
  async connect(): Promise<void> {
    if (this.#closed) {
      return;
    }
    this.client.progress("connecting");
    let connected;
    try {
      connected = await this.#launcher.open(this.#options);
    } catch (error) {
      this.client.fail(error instanceof HostFailure ? error.reason : "unexpected");
      return;
    }
    const { port, handshake, rest, socket } = connected;
    socket.once("close", () => {
      this.client.detach();
      void this.connect();
    });
    this.client.attach(port, handshake, rest);
  }

  close(): void {
    this.#closed = true;
    this.client.close();
  }
}
