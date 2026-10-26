// The e2e build's native dialogs (design 5.9.9): the main process installs
// these stand-ins before anything else, so that no dialog shows or takes the
// focus, not even one asked for while the app starts. Each call is recorded
// in `globalThis.__e2eDialogs` and waits until the test answers it there
// (`tools/e2e/src/dialogs.ts`); the synchronous message box answers
// `cancelId` at once. The main process reads `dialog.showMessageBox` and the
// rest when it calls them, so every call comes here. e2e builds only.

import { dialog, type MessageBoxOptions, type OpenDialogOptions } from "electron";

export function installDialogStandIns(): void {
  const stand = { calls: [] as object[], answers: [] as (((value: unknown) => void) | null)[] };
  Object.assign(globalThis, { __e2eDialogs: stand });
  const message = (kind: string, args: unknown[]) => {
    const options = args.at(-1) as MessageBoxOptions;
    const { type, message, detail, buttons, defaultId, cancelId, checkboxLabel } = options;
    stand.calls.push({ kind, type, message, detail, buttons, defaultId, cancelId, checkboxLabel });
    return options;
  };
  const waiting = <T>() =>
    new Promise<T>((resolve) => stand.answers.push((value) => resolve(value as T)));
  dialog.showMessageBox = (...args: unknown[]) => {
    message("message", args);
    return waiting();
  };
  dialog.showMessageBoxSync = (...args: unknown[]) => {
    stand.answers.push(null);
    return message("message_sync", args).cancelId ?? 0;
  };
  dialog.showOpenDialog = (...args: unknown[]) => {
    const { properties } = args.at(-1) as OpenDialogOptions;
    stand.calls.push({ kind: "open", properties });
    return waiting();
  };
}
