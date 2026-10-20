import type { MethodFixture, MockHost } from "@cupper/host-client/mock";
import type { BrowserShellControls } from "@cupper/shell-api/browser-mock";
import type { ShellInfo } from "@cupper/shell-api";

/** What a screenshot case sets up before the page loads. */
export interface PageFixture {
  /** Overrides of the browser shell's `ShellInfo`: theme, locale, prefs. */
  info: Partial<ShellInfo>;
  /** The mock host's answers (`protocol/fixtures/methods.json` by default). */
  methods: MethodFixture[];
}

declare global {
  interface Window {
    cupperFixture: PageFixture;
    cupperHost: MockHost;
    /** The browser shell's controls: the menu, the system theme and so on. */
    cupperShell: BrowserShellControls;
    /** Every `securitypolicyviolation`, from the first byte on. */
    cupperViolations: string[];
  }
}
