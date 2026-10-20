// The screenshot page: ui-core mounted on the browser ShellAPI, in front of
// the mock host, with the fixture the suite set before loading (`serve.ts`).

import "@cupper/ui-core/styles.css";
import "./fonts.css";

import { createMockShellHost, MockHost } from "@cupper/host-client/mock";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { mount } from "@cupper/ui-core";

const { info, methods, streams } = window.cupperFixture;
const host = new MockHost(methods);
// Every stream starts from a one-page snapshot of the fixture's ops.
host.handle("stream/subscribe", ({ params, conn }) => {
  const { stream } = params as { stream: string };
  const page = { stream, at_seq: 0, epoch: 1, page: 0, last: true };
  queueMicrotask(() => {
    const ops = streams[stream] ?? [];
    conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: { ...page, ops } });
  });
  return { result: { stream, mode: "snapshot", at_seq: 0, epoch: 1, head_seq: 0 } };
});
const { shell, controls } = createBrowserShell({ info, host: createMockShellHost(host) });
window.cupperHost = host;
window.cupperShell = controls;
await mount(shell);
