// The main process (design 5.7.2): the app, its window, the hardening
// baseline (design 5.7.8), the control connection to the host, started
// before the app is ready (design 5.5.1), the quit check (NFR-8) and, in
// keyed builds, restarting a host it cannot verify (design 5.8.1). The window
// bridge (T076) builds on this.

import { mkdirSync } from "node:fs";
import { join } from "node:path";

import { createI18n } from "@cupper/i18n";
import {
  app,
  autoUpdater,
  BrowserWindow,
  Menu,
  powerMonitor,
  protocol,
  session,
  shell,
} from "electron";

import { isReleaseLike } from "../shared/flavor";
import { COMMAND_CHANNEL } from "../shared/ipc";
import { startControl } from "./host";
import { confirmQuit, confirmRestart, tellRestartRefused, type Translate } from "./host/dialogs";
import { QuitGuard, type QuitHost } from "./host/lifecycle";
import { MainLog } from "./log";
import { menuTemplate } from "./menu";
import { appDirs, e2eDirs, type AppDirs } from "./paths";
import {
  APP_SCHEME,
  APP_SCHEME_PRIVILEGES,
  MAIN_DOCUMENT_CSP,
  appProtocolHandler,
} from "./security/app-protocol";
import { hardenWebContents } from "./security/navigation";
import { restrictPermissions } from "./security/permissions";
import { enforceSwitchAllowlist } from "./security/switches";
import { MAIN_DOCUMENT_URL, mainWindowOptions } from "./window";

const flavor = __CUPPER_FLAVOR__;

// First of all, before Electron acts on any of them (design 5.7.8).
if (isReleaseLike(flavor)) {
  enforceSwitchAllowlist(process.argv.slice(1), (code) => process.exit(code), console.error);
}

const dirs = resolveDirs();
mkdirSync(dirs.userData, { recursive: true, mode: 0o700 });
// Before the single-instance lock, which lives in userData.
app.setPath("userData", dirs.userData);
const log = new MainLog(dirs.logs, { home: app.getPath("home"), echo: flavor === "dev" });

protocol.registerSchemesAsPrivileged([APP_SCHEME_PRIVILEGES]);
app.enableSandbox();

let mainWindow: BrowserWindow | null = null;
let host: ReturnType<typeof startControl> | undefined;
let i18n: ReturnType<typeof createI18n> | undefined;
const t: Translate = (key, values) => (i18n ??= createI18n()).t(key, values);
const quit = new QuitGuard(quitHost, (status) => confirmQuit(t, status));

if (app.requestSingleInstanceLock()) {
  start();
} else {
  log.info("another instance is running; quitting");
  app.quit();
}

function resolveDirs(): AppDirs {
  try {
    if (__CUPPER_FLAVOR__ === "e2e") {
      return e2eDirs(process.env.CUPPER_E2E_DIR);
    }
    return appDirs(__CUPPER_FLAVOR__, {
      appData: app.getPath("appData"),
      home: app.getPath("home"),
    });
  } catch (error) {
    console.error(`cupper: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  }
}

function start() {
  log.info("started", { flavor, version: app.getVersion(), electron: process.versions.electron });
  app.on("web-contents-created", (_event, contents) => hardenWebContents(contents, openExternal));
  // Another launch only brings the window forward (design 5.7.8).
  app.on("second-instance", showMainWindow);
  app.on("activate", showMainWindow);
  // Closing the last window leaves the app running, as on macOS (design 5.7.1).
  app.on("window-all-closed", () => undefined);
  // Before `will-quit`: running turns and waiting requests are asked about.
  app.on("before-quit", (event) => {
    if (!quit.settled) {
      event.preventDefault();
      void quit.decide().then((decision) => decision === "quit" && app.quit());
    }
  });
  app.on("will-quit", () => host?.control.close());
  try {
    const e2eRoot = __CUPPER_FLAVOR__ === "e2e" ? process.env.CUPPER_E2E_DIR : undefined;
    const version = app.getVersion();
    host = startControl({ flavor, dataDir: dirs.data, version, e2eRoot, log });
    host.control.client.onState((state) => {
      const unverified =
        state.status === "failed" && ["unauthenticated", "host_unverified"].includes(state.reason);
      if (unverified && flavor !== "release") {
        const dialogs = { confirm: () => confirmRestart(t), refused: () => tellRestartRefused(t) };
        void app.whenReady().then(() => host?.restart(dialogs));
      }
    });
  } catch (error) {
    log.error("cannot start the host connection", { error: String(error) });
  }
  void app.whenReady().then(ready);
}

function ready() {
  restrictPermissions(session.defaultSession);
  const renderer = join(import.meta.dirname, "../renderer");
  protocol.handle(
    APP_SCHEME,
    appProtocolHandler({ cupper: { root: renderer, csp: MAIN_DOCUMENT_CSP } }),
  );
  // Shutdown, logout and installing an update quit without asking (design 5.7.1).
  powerMonitor.on("shutdown", () => quit.skip());
  autoUpdater.on("before-quit-for-update", () => quit.skip());
  const text = (i18n = createI18n(app.getPreferredSystemLanguages()[0]));
  const template = menuTemplate({
    label: (id, n) => text.t(id, { ns: "command", n }),
    onCommand: (cmd) => BrowserWindow.getFocusedWindow()?.webContents.send(COMMAND_CHANNEL, cmd),
    devTools: flavor === "dev",
  });
  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
  showMainWindow();
}

function showMainWindow() {
  if (!app.isReady()) {
    return;
  }
  if (mainWindow !== null) {
    if (mainWindow.isMinimized()) {
      mainWindow.restore();
    }
    mainWindow.show();
    return;
  }
  const preload = join(import.meta.dirname, "../preload/index.cjs");
  const window = new BrowserWindow(mainWindowOptions(flavor, preload));
  mainWindow = window;
  window.once("ready-to-show", () => window.show());
  window.on("closed", () => {
    mainWindow = null;
  });
  window.webContents.on("preload-error", (_event, _path, error) => {
    log.error("preload failed", { error: error.message });
  });
  window.webContents.on("render-process-gone", (_event, details) => {
    log.error("renderer gone", { reason: details.reason, exitCode: details.exitCode });
  });
  void window.loadURL(MAIN_DOCUMENT_URL);
}

/** `host/status` and `host/exit` on the control connection, each given a few seconds. */
function quitHost(): QuitHost | undefined {
  const client = host?.control.client;
  const within = <T>(ms: number, call: Promise<T>) =>
    Promise.race([
      call,
      new Promise<T>((_, reject) => setTimeout(() => reject(new Error("timeout")), ms)),
    ]);
  return (
    client && {
      status: () => within(2000, client.request("host/status")),
      exit: (force) => within(3000, client.command("host/exit", { force })),
    }
  );
}

function openExternal(url: string) {
  shell.openExternal(url).catch((error: unknown) => {
    log.warn("openExternal failed", { error: String(error) });
  });
}
