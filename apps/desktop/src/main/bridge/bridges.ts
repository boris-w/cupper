// The windows' data connections (design 5.7.1): `ShellAPI.host.open()` from
// a window's main document makes a windowBridge bound to that document
// instance. A navigation of the main frame to another document, the renderer
// going away or the window closing ends it, with its subscriptions on the
// host; the next document opens its own.

import type { Handshake } from "@cupper/shell-api";
import type { MessagePortMain, WebContents, WebFrameMain } from "electron";

import { HOST_PORT } from "../../shared/ipc";
import type { InputGuard } from "../input";
import { MAIN_DOCUMENT_URL } from "../window";
import { WindowBridge, type BridgeDeps, type WindowPort } from "./window-bridge";

const MAIN_DOCUMENT = new URL(MAIN_DOCUMENT_URL);

export interface BridgesDeps extends Omit<BridgeDeps, "input"> {
  /** The window's input guard; a window without one opens no connection. */
  guard(contents: WebContents): InputGuard | undefined;
  /** A new channel: the bridge's end, and the end that goes to the window. */
  channel(): { bridge: WindowPort; window: MessagePortMain };
}

export class WindowBridges {
  readonly #deps: BridgesDeps;
  /** By `webContents.id`. */
  readonly #bridges = new Map<number, WindowBridge>();
  readonly #bound = new WeakSet<WebContents>();

  constructor(deps: BridgesDeps) {
    this.#deps = deps;
  }

  /** `host:open` from `frame` of `contents`: a new connection, its port sent ahead. */
  async open(contents: WebContents, frame: WebFrameMain | null): Promise<Handshake> {
    if (frame === null || frame !== contents.mainFrame || !isMainDocument(frame.url)) {
      throw new Error("only the main document opens a data connection");
    }
    const input = this.#deps.guard(contents);
    if (input === undefined) {
      throw new Error("the window has no input guard");
    }
    const id = contents.id;
    this.close(id);
    this.#bind(contents);
    const { bridge: end, window } = this.#deps.channel();
    const bridge = new WindowBridge(end, { ...this.#deps, input });
    this.#bridges.set(id, bridge);
    try {
      const handshake = await bridge.open();
      if (this.#bridges.get(id) !== bridge) {
        throw new Error("the document went away during the handshake");
      }
      contents.postMessage(HOST_PORT, null, [window]);
      return handshake;
    } catch (error) {
      if (this.#bridges.get(id) === bridge) {
        this.close(id);
      }
      throw error;
    }
  }

  close(id: number): void {
    this.#bridges.get(id)?.close();
    this.#bridges.delete(id);
  }

  /** e2e: drops a window's connection, as a host would, without a reload. */
  disconnect(id: number): void {
    this.#bridges.get(id)?.disconnect();
  }

  #bind(contents: WebContents): void {
    if (this.#bound.has(contents)) {
      return;
    }
    this.#bound.add(contents);
    const id = contents.id;
    contents.on("did-start-navigation", (details) => {
      if (details.isMainFrame && !details.isSameDocument) {
        this.close(id);
      }
    });
    contents.on("render-process-gone", () => this.close(id));
    contents.once("destroyed", () => this.close(id));
  }
}

/** `app://cupper/…`; a custom scheme's URL has no origin to compare. */
export function isMainDocument(url: string): boolean {
  try {
    const { protocol, host } = new URL(url);
    return protocol === MAIN_DOCUMENT.protocol && host === MAIN_DOCUMENT.host;
  } catch {
    return false;
  }
}
