// @vitest-environment jsdom
import { HostClient } from "@cupper/host-client";
import { MockHost } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { DEFAULT_PREFS } from "../layout/layout";
import { UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";
import { Presence } from "./presence";
import { HostContext } from "./providers";

afterEach(cleanup);

describe("the window's presence", () => {
  test("told on each change of focus or page, once per state, and again on a new connection", () => {
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
    const focused = document.hasFocus();
    expect(told()).toEqual([{ foreground: focused, session_id: "s_1" }]);

    act(() => void window.dispatchEvent(new Event(focused ? "blur" : "focus")));
    act(() => void window.dispatchEvent(new Event(focused ? "focus" : "blur")));
    act(() => ui.getState().navigate({ page: "new" }));
    act(() => ui.getState().navigate({ page: "new" }));
    act(() => ui.getState().navigate({ page: "session", sessionId: "s_2" }));
    expect(told().slice(1)).toEqual([
      { foreground: !focused, session_id: "s_1" },
      { foreground: focused, session_id: "s_1" },
      { foreground: focused },
      { foreground: focused, session_id: "s_2" },
    ]);

    act(() => client.detach());
    act(connect);
    expect(told().at(-1)).toEqual({ foreground: focused, session_id: "s_2" });
    expect(told()).toHaveLength(6);
  });
});
