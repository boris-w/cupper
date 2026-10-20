// The layout preferences, kept by the shell (`ShellAPI.prefs`): the startup
// snapshot comes from `ShellInfo.prefs`, changes from any window through
// `onChange`. A drag updates the store at every move and saves once at the
// end. The state lives in the UI store (`state/ui.ts`).
//
// A save comes back through `onChange` too, after the shell has written it,
// and saves come back in the order they were made. While a later save of
// the same key waits for its echo, an earlier echo is stale: applied, it
// would set the width back for a moment as keys repeat on a handle.

import type { ShellAPI, Unsubscribe } from "@cupper/shell-api";

import { createUiStore, type LayoutState, type Route, type UiStore } from "../state/ui";
import { DEFAULT_PREFS, type LayoutPrefs } from "./layout";

export type { LayoutState } from "../state/ui";

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

export class LayoutStore {
  /** The UI store the layout lives in, with the route and the per-session state. */
  readonly ui: UiStore;
  readonly #prefs: ShellAPI["prefs"];
  readonly #off: Unsubscribe;
  /** Per key, the saves whose echo has not come back yet, oldest first. */
  readonly #unheard = new Map<string, number[]>();
  #saves = 0;

  constructor(prefs: ShellAPI["prefs"], snapshot: Record<string, unknown>, route?: Route) {
    this.#prefs = prefs;
    this.ui = createUiStore({ prefs: readPrefs(snapshot), overlayOpen: false }, route);
    this.#off = prefs.onChange((key, value) => {
      const unheard = this.#unheard.get(key) ?? [];
      if (unheard.shift() !== undefined && unheard.length > 0) {
        return;
      }
      const field = FIELDS.find((name) => PREF_KEYS[name] === key);
      if (field && VALID[field](value)) {
        this.#update({ prefs: { ...this.getState().prefs, [field]: value } });
      }
    });
  }

  readonly subscribe = (listener: () => void): Unsubscribe => this.ui.subscribe(listener);

  readonly getState = (): LayoutState => this.ui.getState().layout;

  /** Sets a preference; `save: false` keeps it in memory, as during a drag. */
  set<K extends keyof LayoutPrefs>(field: K, value: LayoutPrefs[K], save = true): void {
    this.#update({ prefs: { ...this.getState().prefs, [field]: value } });
    if (save) {
      const [key, n] = [PREF_KEYS[field], ++this.#saves];
      this.#unheard.set(key, [...(this.#unheard.get(key) ?? []), n]);
      this.#prefs.set(key, value).catch((err: unknown) => {
        // No echo comes for it.
        this.#unheard.set(key, this.#unheard.get(key)?.filter((m) => m !== n) ?? []);
        console.warn(`layout: cannot save ${key}`, err);
      });
    }
  }

  setOverlay(open: boolean): void {
    if (open !== this.getState().overlayOpen) {
      this.#update({ overlayOpen: open });
    }
  }

  /** ⌘B: the floating sidebar in a narrow window, the docked one otherwise. */
  toggleSidebar(narrow: boolean): void {
    if (narrow) {
      this.setOverlay(!this.getState().overlayOpen);
    } else {
      this.set("sidebarOpen", !this.getState().prefs.sidebarOpen);
    }
  }

  toggleSidePanel(): void {
    this.set("sidePanelOpen", !this.getState().prefs.sidePanelOpen);
  }

  close(): void {
    this.#off();
  }

  #update(patch: Partial<LayoutState>): void {
    this.ui.setState((state) => ({ layout: { ...state.layout, ...patch } }));
  }
}
