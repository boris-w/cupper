// For the review panel's tests only: `node` in the app's providers, in front of
// a mock host on session s_1, whose streams start empty. `handle` sets the
// host's answers before anything is asked.

import { HostClient } from "@cupper/host-client";
import { MockHost } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { render } from "@testing-library/react";
import type { ReactNode } from "react";

import { HostContext, I18nProvider, ShellProvider } from "../app/providers";
import { TooltipProvider } from "../components/Tooltip";
import { ToasterProvider } from "../feedback/toaster";
import { DEFAULT_PREFS } from "../layout/layout";
import { StateProvider, UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";

export async function mountOnHost(node: ReactNode, handle: (mock: MockHost) => void) {
  const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
  mock.handle("stream/subscribe", ({ params }) => {
    const { stream } = params as { stream: string };
    return { result: { stream, mode: "snapshot", at_seq: 0, head_seq: 0, epoch: 1 } };
  });
  handle(mock);
  const client = new HostClient();
  const conn = mock.connect();
  conn.initialized = true;
  client.attach(conn.port(), {} as Handshake);
  const { shell } = createBrowserShell();
  const info = await shell.init();
  const ui = createUiStore(
    { prefs: DEFAULT_PREFS, overlayOpen: false },
    { page: "session", sessionId: "s_1" },
  );
  render(
    <ShellProvider api={shell} info={info}>
      <I18nProvider locale="zh-CN">
        <ToasterProvider>
          <HostContext value={{ client, state: client.state, handshake: undefined }}>
            <UiProvider ui={ui}>
              <StateProvider>
                <TooltipProvider>{node}</TooltipProvider>
              </StateProvider>
            </UiProvider>
          </HostContext>
        </ToasterProvider>
      </I18nProvider>
    </ShellProvider>,
  );
  return { mock, ui };
}
