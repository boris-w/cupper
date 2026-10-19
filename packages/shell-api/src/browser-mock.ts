// The browser implementation of ShellAPI, for the Playwright screenshot tests
// and unit tests (design 5.7.3). Nothing happens on its own: the test drives
// it through `controls`, playing the menu, the system and the main process.
// It needs no DOM, so it runs in Node as well.

import { isExternalUrl } from "./links";
import type {
  NavigateTarget,
  ShellAPI,
  ShellCommand,
  ShellInfo,
  Theme,
  Unsubscribe,
  UpdateState,
} from "./shell-api";

export interface BrowserShellOptions {
  /** Overrides of the default `ShellInfo`. */
  info?: Partial<ShellInfo>;
}

export interface BrowserShellControls {
  /** A menu accelerator. Any ID goes through, known or not. */
  command(cmd: ShellCommand): void;
  navigate(target: NavigateTarget): void;
  setTheme(theme: Theme): void;
  setFocus(focused: boolean, at: number): void;
  setUpdateState(state: UpdateState): void;
  /** What the next `pickDirectory` resolves to; `null`, the default, is a cancelled picker. */
  setPickedDirectory(path: string | null): void;
  /** The URLs `openExternal` took, in order. */
  readonly openedUrls: readonly string[];
}

export interface BrowserShell {
  shell: ShellAPI;
  controls: BrowserShellControls;
}

const DEFAULT_INFO: ShellInfo = {
  shell: "browser",
  flavor: "dev",
  appVersion: "0.0.0",
  locale: "zh-CN",
  theme: "light",
  prefs: {},
  updateState: { status: "idle" },
  // The review document comes in M1b; a test that loads it passes its URL.
  reviewDocumentUrl: "about:blank",
};

/** Subscribers of one event. A callback removed during an emit is not called. */
class Listeners<Args extends unknown[]> {
  readonly #entries = new Set<{ cb: (...args: Args) => void }>();

  add(cb: (...args: Args) => void): Unsubscribe {
    const entry = { cb };
    this.#entries.add(entry);
    return () => {
      this.#entries.delete(entry);
    };
  }

  emit(...args: Args): void {
    for (const entry of [...this.#entries]) {
      if (this.#entries.has(entry)) {
        entry.cb(...args);
      }
    }
  }
}

export function createBrowserShell(options: BrowserShellOptions = {}): BrowserShell {
  // Values cross the mock the way they cross `contextBridge`: as copies.
  const info: ShellInfo = structuredClone({ ...DEFAULT_INFO, ...options.info });
  const commands = new Listeners<[ShellCommand]>();
  const navigations = new Listeners<[NavigateTarget]>();
  const themes = new Listeners<[Theme]>();
  const focus = new Listeners<[boolean, number]>();
  const prefChanges = new Listeners<[string, unknown]>();
  const updateStates = new Listeners<[UpdateState]>();
  const openedUrls: string[] = [];
  let pickedDirectory: string | null = null;

  const setUpdateState = (state: UpdateState) => {
    info.updateState = structuredClone(state);
    updateStates.emit(structuredClone(state));
  };

  const shell: ShellAPI = {
    init: () => Promise.resolve(structuredClone(info)),
    onCommand: (cb) => commands.add(cb),
    onNavigate: (cb) => navigations.add(cb),
    openExternal: (url) => {
      if (isExternalUrl(url)) {
        openedUrls.push(url);
      }
    },
    pickDirectory: () => Promise.resolve(pickedDirectory),
    system: { onThemeChange: (cb) => themes.add(cb) },
    window: { onFocusChange: (cb) => focus.add(cb) },
    prefs: {
      // `async`: a value that cannot be copied rejects instead of throwing.
      set: async (key, value) => {
        const stored = structuredClone(value);
        info.prefs[key] = stored;
        prefChanges.emit(key, structuredClone(stored));
      },
      onChange: (cb) => prefChanges.add(cb),
    },
    updates: {
      onState: (cb) => updateStates.add(cb),
      installNow: () => {
        const state = info.updateState;
        if (state.status === "ready") {
          setUpdateState({ status: "installing", version: state.version });
        }
      },
    },
  };

  const controls: BrowserShellControls = {
    command: (cmd) => {
      commands.emit(structuredClone(cmd));
    },
    navigate: (target) => {
      navigations.emit(structuredClone(target));
    },
    setTheme: (theme) => {
      info.theme = theme;
      themes.emit(theme);
    },
    setFocus: (focused, at) => {
      focus.emit(focused, at);
    },
    setUpdateState,
    setPickedDirectory: (path) => {
      pickedDirectory = path;
    },
    openedUrls,
  };

  return { shell, controls };
}
