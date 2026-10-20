import type { ConnectionState, Handshake } from "@cupper/shell-api";

/** IPC channels between the main process and the preload. */

/** Main → renderer: a menu command, delivered through `ShellAPI.onCommand`. */
export const COMMAND_CHANNEL = "shell:command";

/** Renderer → main: `ShellInfo` (`ShellAPI.init`). */
export const SHELL_INIT = "shell:init";
/** Renderer → main: `ShellAPI.prefs.set`. */
export const PREFS_SET = "prefs:set";
/** Main → renderer: a preference changed, in any window. */
export const PREFS_CHANGED = "prefs:changed";
/** Renderer → main: `ShellAPI.openExternal`. */
export const OPEN_EXTERNAL = "shell:open-external";
/** Renderer → main: `ShellAPI.pickDirectory`. */
export const PICK_DIRECTORY = "shell:pick-directory";
/** Main → renderer: the system's appearance. */
export const THEME_CHANGED = "shell:theme";
/** Main → renderer: the window's focus, and when it changed. */
export const FOCUS_CHANGED = "shell:focus";
/** Main → renderer: deep links and notification clicks (their tasks send on it). */
export const NAVIGATE = "shell:navigate";
/** Main → renderer: the updater's state (the updater's task sends on it). */
export const UPDATE_STATE = "updates:state";

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
