/** IPC channels between the main process and the preload. */

/** Main → renderer: a menu command, delivered through `ShellAPI.onCommand`. */
export const COMMAND_CHANNEL = "shell:command";
