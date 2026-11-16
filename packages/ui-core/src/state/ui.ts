// The UI state of design 5.7.6 in one zustand store: the page on screen, the
// layout preferences, and per session the composer's draft and attachments
// and the scroll position. These outlive the session's stream subscription.

import type { PromptBlock } from "@cupper/protocol";
import { createStore, type StoreApi } from "zustand/vanilla";

import type { LayoutPrefs } from "../layout/layout";

/** A page of the app proper: a new session, or a session. */
export type AppRoute = { page: "new"; projectId?: string } | { page: "session"; sessionId: string };

/**
 * The page in the main area; T084's shape. The settings page names its
 * section (the first when absent), an item of it to bring into view, and the
 * page "返回应用" goes back to (T151).
 */
export type Route =
  AppRoute | { page: "settings"; section?: string; focus?: string; from: AppRoute };

/** The settings page at `section` from `route`, coming from where `route` came from. */
export function toSettings(route: Route, section?: string, focus?: string): Route {
  return { page: "settings", section, focus, from: route.page === "settings" ? route.from : route };
}

export interface LayoutState {
  prefs: LayoutPrefs;
  /** The floating sidebar of a narrow window; not saved. */
  overlayOpen: boolean;
}

export interface UiState {
  route: Route;
  layout: LayoutState;
  drafts: Partial<Record<string, string>>;
  attachments: Partial<Record<string, readonly PromptBlock[]>>;
  /** The message stream's scroll position, from the top. */
  scroll: Partial<Record<string, number>>;
  /** A composer menu something outside it opened (`change_model`: `model`). */
  menu: string | null;
  /** The one-time hints each session has shown (design 5.7.4); not saved. */
  hinted: Partial<Record<string, readonly string[]>>;
  /** How many steers each session waits on the host's answer to. */
  steering: Partial<Record<string, number>>;
  /** How many messages the user sent in each session from here: the stream goes back to its end on each (T256). */
  sent: Partial<Record<string, number>>;
  /** The projects whose sessions the sidebar shows all of ("展开显示"); not saved. */
  expanded: Partial<Record<string, boolean>>;
  /** The sessions visited, the last first (⌃Tab, design 5.7.7); not saved. */
  recent: readonly string[];
  /** ⌃Tab's walk while ⌃ is held: the recent sessions when it began, and where it is. */
  cycle: { sessions: readonly string[]; at: number } | null;
  navigate(route: Route): void;
  expand(projectId: string, open: boolean): void;
  /**
   * Goes a step through the recent sessions `live` keeps, round the ends;
   * the visits on the way leave the order as it was until `endCycle`.
   */
  cycleRecent(direction: 1 | -1, live: (sessionId: string) => boolean): void;
  /** The walk is over: the session it reached becomes the last visited. */
  endCycle(): void;
  openMenu(menu: string | null): void;
  setDraft(sessionId: string, text: string): void;
  setAttachments(sessionId: string, blocks: readonly PromptBlock[]): void;
  setScroll(sessionId: string, top: number): void;
  /** Marks hint `name` shown in the session; whether it was not yet. */
  hint(sessionId: string, name: string): boolean;
  addSteering(sessionId: string, delta: 1 | -1): void;
  markSent(sessionId: string): void;
}

export type UiStore = StoreApi<UiState>;

export function createUiStore(layout: LayoutState, route: Route = { page: "new" }): UiStore {
  return createStore<UiState>()((set, get) => {
    const per =
      <K extends "drafts" | "attachments" | "scroll">(field: K) =>
      (sessionId: string, value: NonNullable<UiState[K][string]>) =>
        set((state) => ({ [field]: { ...state[field], [sessionId]: value } }));
    return {
      route,
      layout,
      drafts: {},
      attachments: {},
      scroll: {},
      menu: null,
      hinted: {},
      steering: {},
      sent: {},
      expanded: {},
      recent: route.page === "session" ? [route.sessionId] : [],
      cycle: null,
      navigate: (next) => {
        const { recent } = get();
        const visited =
          next.page === "session"
            ? [next.sessionId, ...recent.filter((id) => id !== next.sessionId)]
            : recent;
        set({ route: next, recent: visited, cycle: null });
      },
      expand: (projectId, open) =>
        set((state) => ({ expanded: { ...state.expanded, [projectId]: open } })),
      cycleRecent: (direction, live) => {
        const { route, recent, cycle } = get();
        const sessions = cycle?.sessions ?? recent.filter(live);
        const on = route.page === "session" && route.sessionId === sessions[0];
        const from = cycle?.at ?? (on ? 0 : direction > 0 ? -1 : 0);
        const at = (from + direction + sessions.length) % sessions.length;
        const target = sessions[at];
        if (target === undefined || (on && sessions.length < 2)) return;
        set({ route: { page: "session", sessionId: target }, cycle: { sessions, at } });
      },
      endCycle: () => {
        const { cycle } = get();
        const reached = cycle?.sessions[cycle.at];
        if (reached === undefined) return;
        set((state) => ({
          recent: [reached, ...state.recent.filter((id) => id !== reached)],
          cycle: null,
        }));
      },
      openMenu: (menu) => set({ menu }),
      setDraft: per("drafts"),
      setAttachments: per("attachments"),
      setScroll: per("scroll"),
      hint: (sessionId, name) => {
        const shown = get().hinted[sessionId] ?? [];
        if (shown.includes(name)) return false;
        set({ hinted: { ...get().hinted, [sessionId]: [...shown, name] } });
        return true;
      },
      markSent: (sessionId) =>
        set((state) => ({
          sent: { ...state.sent, [sessionId]: (state.sent[sessionId] ?? 0) + 1 },
        })),
      addSteering: (sessionId, delta) =>
        set((state) => ({
          steering: { ...state.steering, [sessionId]: (state.steering[sessionId] ?? 0) + delta },
        })),
    };
  });
}
