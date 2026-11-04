// What the diff view (T131's review document, or T139's fallback) gets from
// the review panel, and what it may ask of it (design 5.7.5). The view is
// stateless: the panel holds the diff and its token, the drafts, the folded
// files and the place to scroll to, and hands them down; the view hands up
// hunk actions and comment drafts, which the panel checks and carries out.

import type { GitDiffResult, HunkId, PromptBlock, ReviewComment, WirePath } from "@cupper/protocol";
import type { ComponentType } from "react";

import type { UiStore } from "../state/ui";
import type { DiffView, ReviewScope } from "./state";

export type HunkAction = "stage" | "unstage" | "revert";

/**
 * What a hunk or a file offers in each scope: the host stages only from the
 * unstaged diff, unstages only from the staged one, and reverts from the
 * unstaged diff or a turn's. The panel drops any other action the view asks for.
 */
export const ACTIONS: Readonly<Record<ReviewScope, readonly HunkAction[]>> = {
  last_turn: ["revert"],
  unstaged: ["stage", "revert"],
  staged: ["unstage"],
};

/** What a hunk action names: hunks of one file, or whole files, of `diff`. */
export type ViewTarget = { hunk_ids: HunkId[] } | { paths: WirePath[] };

export interface ReviewViewProps {
  sessionId: string;
  scope: ReviewScope;
  /** The diff on screen; every action the view asks for applies to its `diff_token`. */
  diff: GitDiffResult;
  wrap: boolean;
  view: DiffView;
  collapsed: ReadonlySet<string>;
  /** Where to scroll; a new `seq` repeats a jump to the same place. */
  focus?: { path: string; line?: number; seq: number };
  /** The session's comments waiting in the composer, which the view shows on their lines. */
  drafts: readonly ReviewComment[];
  setCollapsed(path: string, collapsed: boolean): void;
  /**
   * Stage and unstage run at once; revert opens the confirmation in the
   * panel, which takes no focus and allows one at a time (RP-11). `label` is
   * the file's path, for the confirmation's title.
   */
  act(action: HunkAction, target: ViewTarget, label?: string): void;
  /** A comment drafted in the view goes to the composer's attachments (GT-2). */
  comment(draft: ReviewComment): void;
}

export type ReviewViewComponent = ComponentType<ReviewViewProps>;

type CommentBlock = ReviewComment & { type: "review_comment" };

export const isComment = (block: PromptBlock): block is CommentBlock =>
  block.type === "review_comment";

/** The session's review comments among its attachments. */
export const commentsOf = (blocks: readonly PromptBlock[] = []): ReviewComment[] =>
  blocks.filter(isComment).map((block) => {
    const comment: Partial<CommentBlock> = { ...block };
    delete comment.type;
    return comment as ReviewComment;
  });

/** Adds a comment draft to the session's attachments, after the others. */
export function addComment(ui: UiStore, sessionId: string, comment: ReviewComment): void {
  const { attachments, setAttachments } = ui.getState();
  setAttachments(sessionId, [
    ...(attachments[sessionId] ?? []),
    { type: "review_comment", ...comment },
  ]);
}
