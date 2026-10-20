// The main document's preload (design 5.7.1, 5.7.3). It runs sandboxed, so it
// is bundled into one CommonJS file and may only require `electron`. The
// ShellAPI goes to the main world as `window.shell`; the renderer gets no
// Electron object, and reaches the host only through the windowBridge's port.

import { SHELL_GLOBAL, type Handshake } from "@cupper/shell-api";
import { contextBridge, ipcRenderer, type IpcRendererEvent } from "electron";

import { HOST_OPEN, HOST_PORT, type ToWindow } from "../shared/ipc";
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

contextBridge.exposeInMainWorld(SHELL_GLOBAL, { host: bridgeHost(openHost) });
