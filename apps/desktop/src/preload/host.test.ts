import type { Handshake } from "@cupper/shell-api";
import { expect, it } from "vitest";

import { bridgeHost, type BridgePort } from "./host";

it("the port's methods are its own properties, which contextBridge copies", async () => {
  const port: BridgePort = {
    post: () => undefined,
    onMessage: () => undefined,
    close: () => undefined,
  };
  const host = bridgeHost(async () => ({ port, handshake: {} as Handshake }));
  const opened = await host.open();
  expect(Object.keys(opened.port).sort()).toEqual(["ack", "close", "onData", "send", "sendBlob"]);
});
