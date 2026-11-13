// @vitest-environment jsdom
import { HostClient } from "@cupper/host-client";
import { MockHost } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { DEFAULT_PREFS } from "../layout/layout";
import { UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";
import { Presence } from "./presence";
import { HostContext } from "./providers";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
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
});
