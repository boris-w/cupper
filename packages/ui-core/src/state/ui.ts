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
  navigate(route: Route): void;
  openMenu(menu: string | null): void;
  setDraft(sessionId: string, text: string): void;
  setAttachments(sessionId: string, blocks: readonly PromptBlock[]): void;
  setScroll(sessionId: string, top: number): void;
}

export type UiStore = StoreApi<UiState>;

export function createUiStore(layout: LayoutState, route: Route = { page: "new" }): UiStore {
  return createStore<UiState>()((set) => {
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
      navigate: (next) => set({ route: next }),
      openMenu: (menu) => set({ menu }),
      setDraft: per("drafts"),
      setAttachments: per("attachments"),
      setScroll: per("scroll"),
    };
  });
}
