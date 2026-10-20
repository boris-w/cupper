import type { ConnectionState, Handshake } from "@cupper/shell-api";

/** IPC channels between the main process and the preload. */

/** Main → renderer: a menu command, delivered through `ShellAPI.onCommand`. */
export const COMMAND_CHANNEL = "shell:command";

/** Renderer → main: opens this document's data connection; the port follows on {@link HOST_PORT}. */
export const HOST_OPEN = "host:open";
/** Main → renderer: the MessagePort of the connection {@link HOST_OPEN} made. */
export const HOST_PORT = "host:port";

/** What the bridge sends the window, in order on one port. */
export type ToWindow =
  | { type: "data"; chunk: ArrayBuffer }
  | { type: "state"; state: ConnectionState; handshake?: Handshake };

/** The preload's HostPort calls, as they cross the port. */
export type FromWindow =
  | { type: "send"; json: string }
  | { type: "blob"; hash: string; offset: number; last: boolean; bytes: ArrayBuffer }
  | { type: "ack"; bytes: number }
  | { type: "close" };
