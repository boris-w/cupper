// The UI state of design 5.7.6 in one zustand store: the page on screen, the
// layout preferences, and per session the composer's draft and attachments
// and the scroll position. These outlive the session's stream subscription.

import type { PromptBlock } from "@cupper/protocol";
import { createStore, type StoreApi } from "zustand/vanilla";

import type { LayoutPrefs } from "../layout/layout";

/** The page in the main area; T084's shape. */
export type Route = { page: "new"; projectId?: string } | { page: "session"; sessionId: string };

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
  navigate(route: Route): void;
  openMenu(menu: string | null): void;
  setDraft(sessionId: string, text: string): void;
  setAttachments(sessionId: string, blocks: readonly PromptBlock[]): void;
  setScroll(sessionId: string, top: number): void;
  /** Marks hint `name` shown in the session; whether it was not yet. */
  hint(sessionId: string, name: string): boolean;
  addSteering(sessionId: string, delta: 1 | -1): void;
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
      navigate: (next) => set({ route: next }),
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
      addSteering: (sessionId, delta) =>
        set((state) => ({
          steering: { ...state.steering, [sessionId]: (state.steering[sessionId] ?? 0) + delta },
        })),
    };
  });
}
