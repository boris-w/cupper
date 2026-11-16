// For the tests of the review panel and the session actions only: `node` in
// the app's providers, in front of a mock host. The window shows `route`,
// session s_1 by default; the index stream starts with `index`, the other
// streams empty. `handle` sets the host's answers before anything is asked.

import { HostClient } from "@cupper/host-client";
import { MockHost } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { render } from "@testing-library/react";
import type { ReactNode } from "react";

import { HostContext, I18nProvider, ShellProvider } from "../app/providers";
import { RouterProvider, type Route } from "../app/router";
import { TooltipProvider } from "../components/Tooltip";
import { ToasterProvider } from "../feedback/toaster";
import { DEFAULT_PREFS } from "../layout/layout";
import { SessionActionsProvider } from "../session/actions";
import { StateProvider, UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";

export async function mountOnHost(
  node: ReactNode,
  handle: (mock: MockHost) => void,
  { index = [], route }: { index?: object[]; route?: Route } = {},
) {
  const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
  mock.handle("stream/subscribe", ({ id, conn, params }) => {
    const { stream } = params as { stream: string };
    if (stream !== "index" || index.length === 0) {
      return { result: { stream, mode: "snapshot", at_seq: 0, head_seq: 0, epoch: 1 } };
    }
    conn.reply(id, { result: { stream, mode: "snapshot", at_seq: 1, head_seq: 1, epoch: 1 } });
    const page = { stream, at_seq: 1, epoch: 1, page: 0, last: true, ops: index };
    conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: page });
    return undefined;
  });
  handle(mock);
  const client = new HostClient();
  const conn = mock.connect();
  conn.initialized = true;
  client.attach(conn.port(), {} as Handshake);
  const { shell, controls } = createBrowserShell();
  const info = await shell.init();
  const ui = createUiStore(
    { prefs: DEFAULT_PREFS, overlayOpen: false },
    route ?? { page: "session", sessionId: "s_1" },
  );
  render(
    <ShellProvider api={shell} info={info}>
      <I18nProvider locale="zh-CN">
        <ToasterProvider>
          <HostContext value={{ client, state: client.state, handshake: undefined }}>
            <UiProvider ui={ui}>
              <StateProvider>
                <RouterProvider>
                  <TooltipProvider>
                    <SessionActionsProvider>{node}</SessionActionsProvider>
                  </TooltipProvider>
                </RouterProvider>
              </StateProvider>
            </UiProvider>
          </HostContext>
        </ToasterProvider>
      </I18nProvider>
    </ShellProvider>,
  );
  return { mock, ui, controls };
}
