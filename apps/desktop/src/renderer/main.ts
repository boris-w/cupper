// The main document: `@cupper/ui-core` on the Electron ShellAPI, which the
// preload puts on the main world's global object (design 5.1.2, 5.7.3). In
// e2e builds only, `window.__cupperTest` reads the host client's sync
// counters and state (design 5.9.9); other builds do not contain it.

import "@cupper/ui-core/styles.css";

import type { HostClient } from "@cupper/host-client";
import { SHELL_GLOBAL, type ShellAPI } from "@cupper/shell-api";
import { mount } from "@cupper/ui-core";

const shell = (globalThis as unknown as Record<string, ShellAPI | undefined>)[SHELL_GLOBAL];
if (!shell) {
  throw new Error(`the preload did not expose window.${SHELL_GLOBAL}`);
}
const onClient =
  __CUPPER_FLAVOR__ === "e2e"
    ? (client: HostClient) => {
        const test = { counters: () => client.counters, state: () => client.state };
        Object.assign(globalThis, { __cupperTest: test });
      }
    : undefined;
await mount(shell, undefined, onClient);
