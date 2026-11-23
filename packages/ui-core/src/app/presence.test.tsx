// @vitest-environment jsdom
import { HostClient } from "@cupper/host-client";
import { MockHost } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { DEFAULT_PREFS } from "../layout/layout";
import { UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";
import { Presence, READ_MARK_MS } from "./presence";
import { HostContext } from "./providers";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("the window's presence", () => {
  test("told on each change of focus or page, once per state, and again on a new connection", async () => {
    const hasFocus = vi.spyOn(document, "hasFocus").mockReturnValue(true);
    const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
    const client = new HostClient({ log: () => undefined });
    const connect = () => {
      const conn = mock.connect();
      conn.initialized = true;
      client.attach(conn.port(), {} as Handshake);
    };
    connect();
    const ui = createUiStore(
      { prefs: DEFAULT_PREFS, overlayOpen: false },
      { page: "session", sessionId: "s_1" },
    );
    render(
      <HostContext value={{ client, state: client.state, handshake: undefined }}>
        <UiProvider ui={ui}>
          <Presence />
        </UiProvider>
      </HostContext>,
    );
    const told = () =>
      mock.requests.filter((r) => r.method === "client/presence").map((r) => r.params);
    expect(told()).toEqual([{ foreground: true, session_id: "s_1" }]);
    const blur = () =>
      act(async () => {
        window.dispatchEvent(new Event("blur"));
        await new Promise((done) => setTimeout(done, 0));
      });

    // Into a frame of the window, the review document's: a blur, the window still in front.
    await blur();
    expect(told()).toHaveLength(1);
    hasFocus.mockReturnValue(false);
    await blur();
    hasFocus.mockReturnValue(true);
    act(() => void window.dispatchEvent(new Event("focus")));
    act(() => ui.getState().navigate({ page: "new" }));
    act(() => ui.getState().navigate({ page: "new" }));
    act(() => ui.getState().navigate({ page: "session", sessionId: "s_2" }));
    expect(told().slice(1)).toEqual([
      { foreground: false, session_id: "s_1" },
      { foreground: true, session_id: "s_1" },
      { foreground: true },
      { foreground: true, session_id: "s_2" },
    ]);

    act(() => client.detach());
    act(connect);
    expect(told().at(-1)).toEqual({ foreground: true, session_id: "s_2" });
    expect(told()).toHaveLength(6);
  });

  test("read marks: the session in front only, up to its seq, at most once a second", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const hasFocus = vi.spyOn(document, "hasFocus").mockReturnValue(true);
    const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
    mock.handle("stream/subscribe", ({ id, conn, params }) => {
      const { stream } = params as { stream: string };
      conn.reply(id, { result: { stream, mode: "snapshot", at_seq: 5, head_seq: 5, epoch: 1 } });
      const page = { stream, at_seq: 5, epoch: 1, page: 0, last: true, ops: [] };
      conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: page });
      return undefined;
    });
    mock.handle("session/mark_read", () => ({ result: {} }));
    const client = new HostClient({ log: () => undefined });
    const conn = mock.connect();
    conn.initialized = true;
    client.attach(conn.port(), {} as Handshake);
    const sink = { reset: () => undefined, apply: () => undefined, live: () => undefined };
    ["session/s_1", "session/s_2"].forEach((stream) => client.subscribe(stream, sink));
    const seqs = new Map<string, number>();
    const event = (stream: string) => {
      const seq = (seqs.get(stream) ?? 5) + 1;
      seqs.set(stream, seq);
      const body = { type: "item.delta", data: { item_id: "i_1", field: "text", delta: "a" } };
      const events = [{ seq, v: 1, ts: 1, ...body }];
      act(() => conn.send({ jsonrpc: "2.0", method: "stream/events", params: { stream, events } }));
    };
    const ui = createUiStore(
      { prefs: DEFAULT_PREFS, overlayOpen: false },
      { page: "session", sessionId: "s_1" },
    );
    render(
      <HostContext value={{ client, state: client.state, handshake: undefined }}>
        <UiProvider ui={ui}>
          <Presence />
        </UiProvider>
      </HostContext>,
    );
    const marks = () =>
      mock.requests.filter((r) => r.method === "session/mark_read").map((r) => r.params);
    // In front: up to the snapshot's seq at once, then a second's events in one.
    expect(marks()).toEqual([{ session_id: "s_1", up_to_seq: 5 }]);
    [1, 2, 3].forEach(() => event("session/s_1"));
    expect(marks()).toHaveLength(1);
    act(() => void vi.advanceTimersByTime(READ_MARK_MS));
    expect(marks().at(-1)).toEqual({ session_id: "s_1", up_to_seq: 8 });
    // Leaving the front tells what came meanwhile; nothing after.
    event("session/s_1");
    hasFocus.mockReturnValue(false);
    act(() => void window.dispatchEvent(new Event("blur")));
    act(() => void vi.advanceTimersByTime(READ_MARK_MS));
    expect(marks().at(-1)).toEqual({ session_id: "s_1", up_to_seq: 9 });
    event("session/s_1");
    // Back in front on another session: that one, not the other.
    act(() => ui.getState().navigate({ page: "session", sessionId: "s_2" }));
    hasFocus.mockReturnValue(true);
    act(() => void window.dispatchEvent(new Event("focus")));
    act(() => void vi.advanceTimersByTime(READ_MARK_MS));
    event("session/s_1");
    expect(marks().slice(2)).toEqual([
      { session_id: "s_1", up_to_seq: 9 },
      { session_id: "s_2", up_to_seq: 5 },
    ]);
  });
});
