// The screenshot page: ui-core mounted on the browser ShellAPI, in front of
// the mock host, with the fixture the suite set before loading (`serve.ts`).

import "@cupper/ui-core/styles.css";
import "./fonts.css";

import { createMockShellHost, MockHost } from "@cupper/host-client/mock";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { mount } from "@cupper/ui-core";

const { info, methods } = window.cupperFixture;
const host = new MockHost(methods);
const { shell, controls } = createBrowserShell({ info, host: createMockShellHost(host) });
window.cupperHost = host;
window.cupperShell = controls;
await mount(shell);
