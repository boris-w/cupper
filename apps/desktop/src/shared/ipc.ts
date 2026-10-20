import type { ConnectionState, Handshake } from "@cupper/shell-api";

/** IPC channels between the main process and the preload. */

/** Main → renderer: a menu command, delivered through `ShellAPI.onCommand`. */
export const COMMAND_CHANNEL = "shell:command";

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
