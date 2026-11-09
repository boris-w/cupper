// The review panel's shared state (design 5.7.5, T130): the scope on screen,
// how the diff shows, which files are collapsed and which one to scroll to.
// The header, the file tree (T232) and the diff view (T131) all read the same
// store and act on the same `git/diff`, so on the same `diff_token`.

import type { GitDiffResult, GitStatusResult } from "@cupper/protocol";
import { useQuery } from "@tanstack/react-query";
import { createContext, use } from "react";
import { createStore, type StoreApi } from "zustand/vanilla";

import { required, useHost } from "../app/providers";

/** The scopes of the "查阅来源" menu in M1 (design 5.7.5, Q13). */
export const SCOPES = ["last_turn", "unstaged", "staged"] as const;
/** Those of the menu, and `turn`: one turn's changes, which its change card opens (T132). */
export type ReviewScope = (typeof SCOPES)[number] | "turn";
/** A scope as `git/diff` takes it. */
export type DiffScope = Exclude<ReviewScope, "turn"> | { turn: string };

export type DiffView = "auto" | "split" | "unified";

export interface ReviewState {
  scope: ReviewScope;
  wrap: boolean;
  view: DiffView;
  /** Paths whose diff is folded; "collapse all" puts every path of the diff here. */
  collapsed: ReadonlySet<string>;
  /** Where the diff view scrolls; a new `seq` repeats a jump to the same place. */
  focus?: { path: string; line?: number; seq: number };
  /** The turn of the `turn` scope. */
  turn?: { sessionId: string; id: string; no: number };
  set(patch: Partial<Omit<ReviewState, "set" | "show">>): void;
  /** A turn's changes, scrolled to `path`. */
  show(target: ReviewTarget): void;
}

export interface ReviewTarget {
  sessionId: string;
  turnId: string;
  turnNo: number;
  path?: string;
}

export type ReviewStore = StoreApi<ReviewState>;

export function createReviewStore(): ReviewStore {
  return createStore<ReviewState>()((set) => ({
    scope: "unstaged",
    wrap: false,
    view: "auto",
    collapsed: new Set(),
    set: (patch) => set(patch),
    show: ({ sessionId, turnId, turnNo, path }) =>
      set(({ focus }) => ({
        scope: "turn",
        turn: { sessionId, id: turnId, no: turnNo },
        ...(path ? { focus: { path, seq: (focus?.seq ?? 0) + 1 } } : {}),
      })),
  }));
}

/** Opens the side panel on a turn's changes; the shell gives it (T132). */
export const OpenReview = createContext<(target: ReviewTarget) => void>(() => undefined);

/** The scope on screen as `git/diff` takes it. */
export const diffScope = ({ scope, turn }: ReviewState): DiffScope =>
  scope === "turn" ? { turn: turn?.id ?? "" } : scope;

export const ReviewContext = createContext<ReviewStore | null>(null);
export const useReviewStore = () => required(use(ReviewContext), "ReviewContext");

/** The query keys of a session's git data, which `turn.completed` makes stale (state/query.ts). */
export const diffKey = (sessionId: string, scope: DiffScope) =>
  [
    "session",
    sessionId,
    "git/diff",
    typeof scope === "string" ? scope : `turn:${scope.turn}`,
  ] as const;
export const statusKey = (sessionId: string) => ["session", sessionId, "git/status"] as const;

export function useDiff(sessionId: string, scope: DiffScope, enabled = true) {
  const { client } = useHost();
  return useQuery({
    enabled,
    queryKey: diffKey(sessionId, scope),
    queryFn: () => client.request<GitDiffResult>("git/diff", { session_id: sessionId, scope }),
  });
}

/** `git/status`, with the paths whose executable configuration changed this session. */
export function useStatus(sessionId: string, enabled = true) {
  const { client } = useHost();
  return useQuery({
    queryKey: statusKey(sessionId),
    queryFn: () => client.request<GitStatusResult>("git/status", { session_id: sessionId }),
    enabled,
  });
}
