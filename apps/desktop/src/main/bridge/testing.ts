// Shared by the bridge's tests: a windowBridge in front of a MockHost, with
// the preload's `ShellAPI.host` on the other end of a Node MessageChannel.
import { readFileSync } from "node:fs";
import { MessageChannel, type MessagePort } from "node:worker_threads";

import { handshake } from "@cupper/host-client";
import { MOCK_HANDSHAKE, MockHost, type MethodFixture } from "@cupper/host-client/mock";

import { bridgeHost, type BridgePort } from "../../preload/host";
import {
  WindowBridge,
  type BridgeDeps,
  type DataConnection,
  type WindowPort,
} from "./window-bridge";

const methods = new URL("../../../../../protocol/fixtures/methods.json", import.meta.url);
export const fixtures = JSON.parse(readFileSync(methods, "utf8")) as MethodFixture[];

/** A data connection to `host`, with its reads paused or not. */
export async function mockConnect(host: MockHost): Promise<DataConnection & { paused(): boolean }> {
  const conn = host.connect();
  const { handshake: done, rest } = await handshake(conn.port(), MOCK_HANDSHAKE);
  let paused = false;
  return {
    port: conn.port(),
    handshake: done,
    rest,
    pause: () => void (paused = true),
    resume: () => void (paused = false),
    onClose: (cb) => void conn.onClose(cb),
    paused: () => paused,
  };
}

const mainEnd = (port: MessagePort): WindowPort => ({
  post: (message) => port.postMessage(message),
  onMessage: (cb) => void port.on("message", cb),
  close: () => port.close(),
});

const windowEnd = (port: MessagePort): BridgePort => ({
  post: (message) => port.postMessage(message),
  onMessage: (cb) => void port.on("message", cb),
  close: () => port.close(),
});

/** A bridge on a MessageChannel, and the preload's end of it. */
export function wired(host: MockHost, deps: Partial<BridgeDeps> = {}) {
  const bridges: WindowBridge[] = [];
  const shell = bridgeHost(async () => {
    const { port1, port2 } = new MessageChannel();
    const bridge = new WindowBridge(mainEnd(port1), {
      connect: () => mockConnect(host),
      controlReady: async () => undefined,
      input: { take: () => undefined },
      schedule: (task) => void setTimeout(task, 0),
      ...deps,
    });
    bridges.push(bridge);
    return { port: windowEnd(port2), handshake: await bridge.open() };
  });
  return { shell, bridges };
}
