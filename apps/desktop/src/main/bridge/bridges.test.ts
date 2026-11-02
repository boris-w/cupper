import { EventEmitter } from "node:events";

import { until } from "@cupper/host-client/contract";
import { MockHost } from "@cupper/host-client/mock";
import type { MessagePortMain, WebContents, WebFrameMain } from "electron";
import { describe, expect, test } from "vitest";

import { HOST_PORT } from "../../shared/ipc";
import { MAIN_DOCUMENT_URL } from "../window";
import { WindowBridges, type BridgesDeps } from "./bridges";
import { fixtures, mockConnect } from "./testing";
import { InputGuard } from "../input/guard";

/** A window's webContents, as far as the bridges use it. */
function contents(id: number, reloads: number[] = []) {
  const emitter = new EventEmitter();
  const mainFrame = { url: MAIN_DOCUMENT_URL } as WebFrameMain;
  const sent: unknown[][] = [];
  const postMessage = (...args: unknown[]) => void sent.push(args);
  const more = {
    id,
    mainFrame,
    postMessage,
    isDestroyed: () => false,
    reload: () => reloads.push(id),
  };
  return { it: Object.assign(emitter, more) as unknown as WebContents, emitter, sent };
}

function bridges(host: MockHost, deps: Partial<BridgesDeps> = {}) {
  return new WindowBridges({
    connect: () => mockConnect(host),
    controlReady: async () => undefined,
    schedule: (task) => void setTimeout(task, 0),
    guard: () => new InputGuard(() => 0),
    channel: () => ({
      bridge: { post: () => undefined, onMessage: () => undefined, close: () => undefined },
      window: {} as MessagePortMain,
    }),
    ...deps,
  });
}

describe("UNIT-bridge-lifecycle", () => {
  test("only the main document opens, and its port goes ahead of the handshake", async () => {
    const host = new MockHost(fixtures);
    const all = bridges(host);
    const window = contents(1);
    const other = { url: "https://example.com/" } as WebFrameMain;
    await expect(all.open(window.it, other)).rejects.toThrow("main document");
    await expect(all.open(window.it, null)).rejects.toThrow("main document");
    expect(host.connections).toEqual([]);
    const handshake = await all.open(window.it, window.it.mainFrame);
    expect(handshake.host).toEqual(host.init.host);
    expect(window.sent).toEqual([[HOST_PORT, null, [{}]]]);
  });

  test("the connection lives as long as its document instance", async () => {
    const host = new MockHost(fixtures);
    const all = bridges(host);
    const window = contents(1);
    const open = () => all.open(window.it, window.it.mainFrame);
    const alive = () => host.connections.map((c) => !c.closed);

    await open();
    window.emitter.emit("did-start-navigation", { isMainFrame: true, isSameDocument: true });
    window.emitter.emit("did-start-navigation", { isMainFrame: false, isSameDocument: false });
    expect(alive()).toEqual([true]);
    window.emitter.emit("did-start-navigation", { isMainFrame: true, isSameDocument: false });
    expect(alive()).toEqual([false]);

    await open();
    await open();
    expect(alive()).toEqual([false, false, true]);
    window.emitter.emit("render-process-gone", {}, { reason: "crashed" });
    await open();
    window.emitter.emit("destroyed");
    await until(() => expect(alive()).toEqual([false, false, false, false]));
  });

  test("windows whose connection failed load again once the control connection is back", async () => {
    // As after restarting a host this app could not prove its key to (T215).
    const host = new MockHost(fixtures);
    let refused = true;
    let controlBack = true;
    const all = bridges(host, {
      connect: () => (refused ? Promise.reject(new Error("unauthenticated")) : mockConnect(host)),
      controlReady: () => (controlBack ? Promise.resolve() : Promise.reject(new Error("failed"))),
    });
    const reloads: number[] = [];
    const open = (id: number) => {
      const window = contents(id, reloads).it;
      return all.open(window, window.mainFrame);
    };
    await expect(open(1)).rejects.toThrow("unauthenticated");
    refused = false;
    await open(2);
    await open(3);
    // Window 2's connection drops and does not come back; window 3's stays.
    controlBack = false;
    host.connections[0]?.close();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(reloads).toEqual([]);
    all.reload();
    expect(reloads).toEqual([1, 2]);
    all.reload();
    expect(reloads).toEqual([1, 2]);
  });
});
