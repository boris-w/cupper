// The main process's side of the ShellAPI (design 5.7.3) besides the data
// connection: the static facts, the preferences, links, the directory picker,
// and the appearance the windows follow; the focus comes with the input guard. Only a window's main
// document is answered.

import { isExternalUrl, type ShellInfo } from "@cupper/shell-api";
import {
  app,
  BrowserWindow,
  dialog,
  ipcMain,
  nativeTheme,
  type IpcMainEvent,
  type IpcMainInvokeEvent,
} from "electron";

import type { Flavor } from "../shared/flavor";
import {
  OPEN_EXTERNAL,
  PICK_DIRECTORY,
  PREFS_CHANGED,
  PREFS_SET,
  SHELL_INIT,
  THEME_CHANGED,
} from "../shared/ipc";
import { isMainDocument } from "./bridge/bridges";
import type { Prefs } from "./prefs";

/** Where the review document loads from (M1b, design 5.7.5). */
const REVIEW_DOCUMENT_URL = "app://cupper-review/index.html";

const fromMainDocument = (event: IpcMainEvent | IpcMainInvokeEvent) =>
  event.senderFrame !== null &&
  event.senderFrame === event.sender.mainFrame &&
  isMainDocument(event.senderFrame.url);

function checked<Args extends unknown[], R>(
  handler: (event: IpcMainInvokeEvent, ...args: Args) => R,
): (event: IpcMainInvokeEvent, ...args: Args) => R {
  return (event, ...args) => {
    if (!fromMainDocument(event)) {
      throw new Error("not the main document");
    }
    return handler(event, ...args);
  };
}

const theme = () => (nativeTheme.shouldUseDarkColors ? "dark" : "light");

function broadcast(channel: string, ...args: unknown[]): void {
  BrowserWindow.getAllWindows().forEach((window) => window.webContents.send(channel, ...args));
}

export function serveShell(options: {
  flavor: Flavor;
  prefs: Prefs;
  openExternal: (url: string) => void;
}): void {
  const { flavor, prefs, openExternal } = options;
  ipcMain.handle(
    SHELL_INIT,
    checked((): ShellInfo => ({
      shell: "electron",
      flavor,
      appVersion: app.getVersion(),
      locale: app.getPreferredSystemLanguages()[0] ?? "en",
      theme: theme(),
      prefs: prefs.snapshot(),
      updateState: { status: "idle" },
      reviewDocumentUrl: REVIEW_DOCUMENT_URL,
    })),
  );
  ipcMain.handle(
    PREFS_SET,
    checked((_event, key: unknown, value: unknown) => prefs.set(String(key), value)),
  );
  prefs.onChange((key, value) => broadcast(PREFS_CHANGED, key, value));
  ipcMain.on(OPEN_EXTERNAL, (event, url: unknown) => {
    if (fromMainDocument(event) && typeof url === "string" && isExternalUrl(url)) {
      openExternal(url);
    }
  });
  ipcMain.handle(
    PICK_DIRECTORY,
    checked(async (event) => {
      const window = BrowserWindow.fromWebContents(event.sender);
      const properties: ("openDirectory" | "createDirectory")[] = [
        "openDirectory",
        "createDirectory",
      ];
      // Taken at the call, so that e2e tests can stand in for it (design 5.7.2).
      const picked = window
        ? await dialog.showOpenDialog(window, { properties })
        : await dialog.showOpenDialog({ properties });
      return picked.canceled ? null : (picked.filePaths[0] ?? null);
    }),
  );
  nativeTheme.on("updated", () => broadcast(THEME_CHANGED, theme()));
}
