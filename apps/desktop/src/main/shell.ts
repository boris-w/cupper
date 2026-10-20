// The main process's side of the ShellAPI (design 5.7.3) besides the data
// connection: the static facts, the preferences, links, the directory picker,
// the privileged actions, and the appearance the windows follow; the focus
// comes with the input guard. Only a window's main document is answered.

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
  PRIVILEGED,
  SHELL_INIT,
  THEME_CHANGED,
} from "../shared/ipc";
import { isMainDocument } from "./bridge/bridges";
import { guardOf } from "./input";
import type { Prefs } from "./prefs";
import type { Privileged, PrivilegedUi } from "./privileged";

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
  /** Without a host connection, there is none. */
  privileged?: Privileged;
}): void {
  const { flavor, prefs, openExternal, privileged } = options;
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
    checked((event) => pickDirectory(BrowserWindow.fromWebContents(event.sender))),
  );
  ipcMain.handle(
    PRIVILEGED,
    checked(async (event, action: unknown) => {
      // The input goes, whatever happens to the request (design 5.7.8, rule 4).
      const input = guardOf(event.sender)?.take();
      if (privileged === undefined) {
        return { status: "failed", error: { code: "host_unreachable" } };
      }
      return privileged.request(
        input,
        action,
        nativeUi(BrowserWindow.fromWebContents(event.sender)),
      );
    }),
  );
  nativeTheme.on("updated", () => broadcast(THEME_CHANGED, theme()));
}

/**
 * The native dialogs on `window`. They are read from `dialog` when called,
 * so that e2e tests can stand in for them (design 5.7.2).
 */
function nativeUi(window: BrowserWindow | null): PrivilegedUi {
  return {
    confirm: (options) =>
      window ? dialog.showMessageBox(window, options) : dialog.showMessageBox(options),
    pickDirectory: () => pickDirectory(window),
  };
}

async function pickDirectory(window: BrowserWindow | null): Promise<string | null> {
  const properties: ("openDirectory" | "createDirectory")[] = ["openDirectory", "createDirectory"];
  const picked = window
    ? await dialog.showOpenDialog(window, { properties })
    : await dialog.showOpenDialog({ properties });
  return picked.canceled ? null : (picked.filePaths[0] ?? null);
}
