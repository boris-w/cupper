// The main process (design 5.7.2): the app, its window, the hardening
// baseline (design 5.7.8) and the control connection to the host, started
// before the app is ready (design 5.5.1). The window bridge (T076) builds on
// this.

import { mkdirSync } from "node:fs";
import { join } from "node:path";

import { createI18n } from "@cupper/i18n";
import {
  app,
  BrowserWindow,
  ipcMain,
  Menu,
  MessageChannelMain,
  protocol,
  session,
  shell,
} from "electron";

import { isReleaseLike } from "../shared/flavor";
import { COMMAND_CHANNEL, HOST_OPEN } from "../shared/ipc";
import { WindowBridges } from "./bridge/bridges";
import { startHost } from "./host";
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
let host: ReturnType<typeof startHost> | undefined;

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
  app.on("will-quit", () => host?.control.close());
  try {
    const e2eRoot = __CUPPER_FLAVOR__ === "e2e" ? process.env.CUPPER_E2E_DIR : undefined;
    const version = app.getVersion();
    host = startHost({ flavor, dataDir: dirs.data, version, e2eRoot, log });
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
  const i18n = createI18n(app.getPreferredSystemLanguages()[0]);
  const template = menuTemplate({
    label: (id, n) => i18n.t(id, { ns: "command", n }),
    onCommand: (cmd) => BrowserWindow.getFocusedWindow()?.webContents.send(COMMAND_CHANNEL, cmd),
    devTools: flavor === "dev",
  });
  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
  if (host) {
    serveWindows(host);
  }
  showMainWindow();
}

/** Each window document's data connection, through its windowBridge (design 5.7.1). */
function serveWindows({ control, openWindow }: NonNullable<typeof host>) {
  const bridges = new WindowBridges({
    connect: openWindow,
    controlReady: () => control.ready(),
    schedule: (task, ms) => void setTimeout(task, ms),
    channel: () => {
      const { port1, port2 } = new MessageChannelMain();
      const bridge = {
        post: (message: unknown) => port1.postMessage(message),
        onMessage: (cb: (message: unknown) => void) => {
          port1.on("message", (event) => cb(event.data));
          port1.start();
        },
        close: () => port1.close(),
      };
      return { bridge, window: port2 };
    },
  });
  ipcMain.handle(HOST_OPEN, (event) => bridges.open(event.sender, event.senderFrame));
  if (__CUPPER_FLAVOR__ === "e2e") {
    // For the e2e tests' `electronApp.evaluate`: a window loses its
    // connection without a reload (E2E-NFR1-4).
    const disconnectWindow = (webContentsId: number) => bridges.disconnect(webContentsId);
    Object.assign(globalThis, { __cupperE2E: { disconnectWindow } });
  }
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

function openExternal(url: string) {
  shell.openExternal(url).catch((error: unknown) => {
    log.warn("openExternal failed", { error: String(error) });
  });
}
