// `ShellAPI.host` in the preload (design 5.7.1, 5.7.3): each `open` gets a
// fresh port from the main process's windowBridge, with the handshake it
// made. The HostPort's calls cross the port as messages; what comes back,
// data and connection states, comes in order on the same port. No Electron
// here, so the HostPort contract runs on it in Node.

import type {
  ConnectionState,
  Handshake,
  HostPort,
  ShellAPI,
  Unsubscribe,
} from "@cupper/shell-api";

import type { FromWindow, ToWindow } from "../shared/ipc";

/** The window's end of the port. */
export interface BridgePort {
  post(message: FromWindow): void;
  onMessage(cb: (message: ToWindow) => void): void;
  close(): void;
}

class Listeners<Args extends unknown[]> {
  readonly #cbs = new Set<(...args: Args) => void>();

  add(cb: (...args: Args) => void): Unsubscribe {
    const entry = (...args: Args) => cb(...args);
    this.#cbs.add(entry);
    return () => void this.#cbs.delete(entry);
  }

  emit(...args: Args): void {
    [...this.#cbs].forEach((cb) => cb(...args));
  }
}

type States = Listeners<[ConnectionState, Handshake | undefined]>;

class WindowHostPort implements HostPort {
  readonly #port: BridgePort;
  readonly #data = new Listeners<[ArrayBuffer]>();
  /** Data that came before the first listener. */
  #backlog: ArrayBuffer[] | undefined = [];
  #closed = false;

  constructor(port: BridgePort, states: States) {
    this.#port = port;
    port.onMessage((message) => {
      if (this.#closed) {
        return;
      }
      if (message.type === "data") {
        if (this.#backlog) {
          this.#backlog.push(message.chunk);
        } else {
          this.#data.emit(message.chunk);
        }
      } else {
        states.emit(message.state, message.handshake);
      }
    });
  }

  send(json: string): void {
    this.#post({ type: "send", json });
  }

  sendBlob(hash: string, offset: number, last: boolean, bytes: ArrayBuffer): void {
    this.#post({ type: "blob", hash, offset, last, bytes });
  }

  onData(cb: (chunk: ArrayBuffer) => void): Unsubscribe {
    const off = this.#data.add(cb);
    const backlog = this.#backlog ?? [];
    this.#backlog = undefined;
    backlog.forEach((chunk) => cb(chunk));
    return off;
  }

  ack(bytes: number): void {
    this.#post({ type: "ack", bytes });
  }

  close(): void {
    this.#post({ type: "close" });
    this.#closed = true;
    this.#port.close();
  }

  #post(message: FromWindow): void {
    if (!this.#closed) {
      this.#port.post(message);
    }
  }
}

/** `open` asks the main process for a port and its handshake. */
export function bridgeHost(
  open: () => Promise<{ port: BridgePort; handshake: Handshake }>,
): ShellAPI["host"] {
  const states: States = new Listeners();
  let current: WindowHostPort | undefined;
  return {
    open: async () => {
      current?.close();
      const { port, handshake } = await open();
      current = new WindowHostPort(port, states);
      return { port: current, handshake };
    },
    onConnectionState: (cb) => states.add(cb),
  };
}
