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
export type ReviewScope = (typeof SCOPES)[number];

export type DiffView = "auto" | "split" | "unified";

export interface ReviewState {
  scope: ReviewScope;
  wrap: boolean;
  view: DiffView;
  /** Paths whose diff is folded; "collapse all" puts every path of the diff here. */
  collapsed: ReadonlySet<string>;
  /** Where the diff view scrolls; a new `seq` repeats a jump to the same place. */
  focus?: { path: string; line?: number; seq: number };
  set(patch: Partial<Omit<ReviewState, "set">>): void;
}

export type ReviewStore = StoreApi<ReviewState>;

export function createReviewStore(): ReviewStore {
  return createStore<ReviewState>()((set) => ({
    scope: "unstaged",
    wrap: false,
    view: "auto",
    collapsed: new Set(),
    set: (patch) => set(patch),
  }));
}

export const ReviewContext = createContext<ReviewStore | null>(null);
export const useReviewStore = () => required(use(ReviewContext), "ReviewContext");

/** The query keys of a session's git data, which `turn.completed` makes stale (state/query.ts). */
export const diffKey = (sessionId: string, scope: ReviewScope) =>
  ["session", sessionId, "git/diff", scope] as const;
export const statusKey = (sessionId: string) => ["session", sessionId, "git/status"] as const;

export function useDiff(sessionId: string, scope: ReviewScope) {
  const { client } = useHost();
  return useQuery({
    queryKey: diffKey(sessionId, scope),
    queryFn: () => client.request<GitDiffResult>("git/diff", { session_id: sessionId, scope }),
  });
}

/** `git/status`, with the paths whose executable configuration changed this session (T123). */
export type ReviewStatus = GitStatusResult & { repo_config_changed?: string[] };

export function useStatus(sessionId: string) {
  const { client } = useHost();
  return useQuery({
    queryKey: statusKey(sessionId),
    queryFn: () => client.request<ReviewStatus>("git/status", { session_id: sessionId }),
  });
}
