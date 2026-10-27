// What the main process does with an error nothing caught (T200): it writes
// it to main.log, stack, cause and all, and runs on. Without a listener of
// its own, Electron shows such an error in `dialog.showErrorBox`, a modal box
// that holds the main thread until someone closes it: no window reconnects
// meanwhile, and the app cannot quit.

import { inspect } from "node:util";

import type { MainLog } from "./log";

/** Logs `uncaughtException` and `unhandledRejection` of `proc`; Electron then shows no box. */
export function catchUncaught(proc: Pick<NodeJS.EventEmitter, "on">, log: Pick<MainLog, "error">) {
  proc.on("uncaughtException", (error: unknown, origin: string) => {
    log.error("uncaught exception", { origin, error: inspect(error) });
  });
  proc.on("unhandledRejection", (reason: unknown) => {
    log.error("unhandled rejection", { error: inspect(reason) });
  });
}
