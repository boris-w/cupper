// The layout preferences, kept by the shell (`ShellAPI.prefs`): the startup
// snapshot comes from `ShellInfo.prefs`, changes from any window through
// `onChange`. A drag updates the store at every move and saves once at the
// end. The state layer of T080 takes this over.

import type { ShellAPI, Unsubscribe } from "@cupper/shell-api";

import { DEFAULT_PREFS, type LayoutPrefs } from "./layout";

/** The preference key of each layout field. */
export const PREF_KEYS: Record<keyof LayoutPrefs, string> = {
  sidebarOpen: "layout.sidebarOpen",
  sidebarWidth: "layout.sidebarWidth",
  sidePanelOpen: "layout.sidePanelOpen",
  sidePanelRatio: "layout.sidePanelRatio",
};

const VALID: { [K in keyof LayoutPrefs]: (value: unknown) => value is LayoutPrefs[K] } = {
  sidebarOpen: (value) => typeof value === "boolean",
  sidebarWidth: (value): value is number => typeof value === "number" && Number.isFinite(value),
  sidePanelOpen: (value) => typeof value === "boolean",
  sidePanelRatio: (value): value is number | null =>
    value === null || (typeof value === "number" && value >= 0 && value <= 1),
};

const FIELDS = Object.keys(PREF_KEYS) as (keyof LayoutPrefs)[];

/** The layout fields of a prefs snapshot; a missing or malformed value is the default. */
export function readPrefs(stored: Record<string, unknown>): LayoutPrefs {
  const prefs = { ...DEFAULT_PREFS };
  for (const field of FIELDS) {
    const value = stored[PREF_KEYS[field]];
    if (VALID[field](value)) {
      Object.assign(prefs, { [field]: value });
    }
  }
  return prefs;
}

export interface LayoutState {
  prefs: LayoutPrefs;
  /** The floating sidebar of a narrow window; not saved. */
  overlayOpen: boolean;
}

export class LayoutStore {
  #state: LayoutState;
  readonly #prefs: ShellAPI["prefs"];
  readonly #listeners = new Set<() => void>();
  readonly #off: Unsubscribe;

  constructor(prefs: ShellAPI["prefs"], snapshot: Record<string, unknown>) {
    this.#prefs = prefs;
    this.#state = { prefs: readPrefs(snapshot), overlayOpen: false };
    this.#off = prefs.onChange((key, value) => {
      const field = FIELDS.find((name) => PREF_KEYS[name] === key);
      if (field && VALID[field](value)) {
        this.#update({ prefs: { ...this.#state.prefs, [field]: value } });
      }
    });
  }

  readonly subscribe = (listener: () => void): Unsubscribe => {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  };

  readonly getState = (): LayoutState => this.#state;

  /** Sets a preference; `save: false` keeps it in memory, as during a drag. */
  set<K extends keyof LayoutPrefs>(field: K, value: LayoutPrefs[K], save = true): void {
    this.#update({ prefs: { ...this.#state.prefs, [field]: value } });
    if (save) {
      this.#prefs.set(PREF_KEYS[field], value).catch((err: unknown) => {
        console.warn(`layout: cannot save ${PREF_KEYS[field]}`, err);
      });
    }
  }

  setOverlay(open: boolean): void {
    if (open !== this.#state.overlayOpen) {
      this.#update({ overlayOpen: open });
    }
  }

  /** ⌘B: the floating sidebar in a narrow window, the docked one otherwise. */
  toggleSidebar(narrow: boolean): void {
    if (narrow) {
      this.setOverlay(!this.#state.overlayOpen);
    } else {
      this.set("sidebarOpen", !this.#state.prefs.sidebarOpen);
    }
  }

  close(): void {
    this.#off();
    this.#listeners.clear();
  }

  #update(patch: Partial<LayoutState>): void {
    this.#state = { ...this.#state, ...patch };
    for (const listener of this.#listeners) {
      listener();
    }
  }
}
