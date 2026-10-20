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
  navigate(route: Route): void;
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
      navigate: (next) => set({ route: next }),
      setDraft: per("drafts"),
      setAttachments: per("attachments"),
      setScroll: per("scroll"),
    };
  });
}
