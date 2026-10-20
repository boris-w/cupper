// The main document: `@cupper/ui-core` on the Electron ShellAPI, which the
// preload puts on the main world's global object (design 5.1.2, 5.7.3).

import "@cupper/ui-core/styles.css";

import { SHELL_GLOBAL, type ShellAPI } from "@cupper/shell-api";
import { mount } from "@cupper/ui-core";

const shell = (globalThis as unknown as Record<string, ShellAPI | undefined>)[SHELL_GLOBAL];
if (!shell) {
  throw new Error(`the preload did not expose window.${SHELL_GLOBAL}`);
}
await mount(shell);
