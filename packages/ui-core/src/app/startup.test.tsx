// @vitest-environment jsdom
import { MockHost } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { App } from "./App";
import { MARKS, Startup } from "./startup";

afterEach(cleanup);

const ORDER = [MARKS.connected, MARKS.index, MARKS.ready];

describe("the start's marks (T262)", () => {
  test("each once; ready once the index and the composer both are, either first", () => {
    for (const composerFirst of [false, true]) {
      const marks: string[] = [];
      const start = new Startup((name) => marks.push(name));
      start.connected();
      start.connected();
      if (composerFirst) start.composerMounted();
      start.indexApplied();
      expect(marks.includes(MARKS.ready)).toBe(composerFirst);
      start.composerMounted();
      start.indexApplied();
      expect(marks).toEqual(ORDER);
    }
  });

  test("the app marks them in order: the window connected, the index applied, ready", async () => {
    const host = new MockHost([{ method: "initialize", params: {}, result: {} }]);
    host.handle("stream/subscribe", ({ params, conn }) => {
      const { stream } = params as { stream: string };
      const page = { stream, at_seq: 0, epoch: 1, page: 0, last: true, ops: [] };
      queueMicrotask(() => conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: page }));
      return { result: { stream, mode: "snapshot", at_seq: 0, epoch: 1, head_seq: 0 } };
    });
    // The connection as the main process hands it over, handshaken.
    const open = async () => {
      const conn = host.connect();
      conn.initialized = true;
      const handshake = { host: { weak_auth: false }, limits: { max_frame_bytes: 64_000 } };
      return { port: conn.port(), handshake: handshake as Handshake };
    };
    const { shell } = createBrowserShell({
      host: { open, onConnectionState: () => () => undefined },
    });
    // What jsdom lacks of the browser the shell lays out in.
    vi.stubGlobal("requestIdleCallback", (cb: () => void) => setTimeout(cb));
    vi.stubGlobal("cancelIdleCallback", (id: number) => clearTimeout(id));
    const mark = vi.spyOn(performance, "mark");
    render(<App api={shell} info={await shell.init()} />);
    const ours = () =>
      mark.mock.calls.map(([name]) => name).filter((name) => name.startsWith("cupper:"));
    await vi.waitFor(() => expect(ours()).toEqual(ORDER));
  });
});
