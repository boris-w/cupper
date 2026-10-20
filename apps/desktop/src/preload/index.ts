// The main document's preload (design 5.7.1, 5.7.3). It runs sandboxed, so it
// is bundled into one CommonJS file and may only require `electron`. The
// ShellAPI goes to the main world as `window.shell`; the renderer gets no
// Electron object, and reaches the host only through the windowBridge's port.
// Navigation and the updater's state come once their senders exist.

import {
  SHELL_GLOBAL,
  type Handshake,
  type PrivilegedResult,
  type ShellAPI,
  type ShellInfo,
  type Unsubscribe,
} from "@cupper/shell-api";
import { contextBridge, ipcRenderer, type IpcRendererEvent } from "electron";

import {
  COMMAND_CHANNEL,
  FOCUS_CHANGED,
  HOST_OPEN,
  HOST_PORT,
  NAVIGATE,
  OPEN_EXTERNAL,
  PICK_DIRECTORY,
  PREFS_CHANGED,
  PREFS_SET,
  PRIVILEGED,
  SHELL_INIT,
  THEME_CHANGED,
  UPDATE_STATE,
  type ToWindow,
} from "../shared/ipc";
import { bridgeHost, type BridgePort } from "./host";

/**
 * A DOM MessagePort, as far as used here; the project compiles without the
 * DOM library, where `MessagePort` is Node's.
 */
interface WebPort {
  postMessage(message: unknown): void;
  onmessage: ((event: { data: ToWindow }) => void) | null;
  close(): void;
}

/** The port the main process sends with the connection `host:open` makes. */
async function openHost() {
  let take: (port: WebPort | undefined) => void = () => undefined;
  const ported = new Promise<WebPort | undefined>((resolve) => (take = resolve));
  const onPort = (event: IpcRendererEvent) => take(event.ports[0] as unknown as WebPort);
  ipcRenderer.once(HOST_PORT, onPort);
  try {
    const handshake = (await ipcRenderer.invoke(HOST_OPEN)) as Handshake;
    const port = await ported;
    if (port === undefined) {
      throw new Error("the main process sent no port");
    }
    const end: BridgePort = {
      post: (message) => port.postMessage(message),
      onMessage: (cb) => (port.onmessage = (event) => cb(event.data)),
      close: () => port.close(),
    };
    return { port: end, handshake };
  } catch (error) {
    ipcRenderer.removeListener(HOST_PORT, onPort);
    throw error;
  }
}

/** Calls `cb` with what the main process sends on `channel`, until unsubscribed. */
function on<Args extends unknown[]>(channel: string, cb: (...args: Args) => void): Unsubscribe {
  const listener = (_event: IpcRendererEvent, ...args: unknown[]) => cb(...(args as Args));
  ipcRenderer.on(channel, listener);
  return () => void ipcRenderer.removeListener(channel, listener);
}

const shell: ShellAPI = {
  init: () => ipcRenderer.invoke(SHELL_INIT) as Promise<ShellInfo>,
  host: bridgeHost(openHost),
  requestPrivileged: (action) =>
    ipcRenderer.invoke(PRIVILEGED, action) as Promise<PrivilegedResult>,
  onCommand: (cb) => on(COMMAND_CHANNEL, cb),
  onNavigate: (cb) => on(NAVIGATE, cb),
  openExternal: (url) => ipcRenderer.send(OPEN_EXTERNAL, url),
  pickDirectory: () => ipcRenderer.invoke(PICK_DIRECTORY) as Promise<string | null>,
  system: { onThemeChange: (cb) => on(THEME_CHANGED, cb) },
  window: { onFocusChange: (cb) => on(FOCUS_CHANGED, cb) },
  prefs: {
    set: async (key, value) => void (await ipcRenderer.invoke(PREFS_SET, key, value)),
    onChange: (cb) => on(PREFS_CHANGED, cb),
  },
  updates: { onState: (cb) => on(UPDATE_STATE, cb), installNow: () => undefined },
};

contextBridge.exposeInMainWorld(SHELL_GLOBAL, shell);
