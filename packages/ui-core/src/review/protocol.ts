// The review document's port protocol (design 5.7.5), version 1: what the
// main document and the review document say to each other over the
// MessageChannel the main document made. Both sides parse every message
// with these schemas and drop one that does not fit. The main document
// holds every state; the review document is a view of it.
//
// Down: `init`, `theme_changed`; `files` (the diff's files, what is folded,
// how they show, the scope's actions and the drafts); `render` (one
// file's hunks, the answer to `need_file`); `scroll_to`; `clear`.
// Up: `ready`, `need_file`, `set_collapsed`, `hunk_action`, `draft_comment`,
// `key_forward`.

import {
  zGitDiffFile,
  zHunkId,
  zReviewComment,
  zWirePath,
  type GitDiffFile,
} from "@cupper/protocol";
import { z } from "zod";

export const REVIEW_PROTOCOL = 1;

const zTheme = z.enum(["light", "dark"]);
const zStrings = z.record(z.string(), z.string());
const zAction = z.enum(["stage", "unstage", "revert"]);
const zToken = z.string().max(64);
/** A hunk with its lines inline: the main document fetched any blob first. */
const zHunk = z.object({
  hunk_id: zHunkId,
  unit: z.string(),
  header: z.string(),
  lines: z.array(z.string()),
});

export const zToReview = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("init"),
    v: z.literal(REVIEW_PROTOCOL),
    theme: zTheme,
    locale: z.string().max(35),
    strings: zStrings,
  }),
  z.object({ type: z.literal("theme_changed"), theme: zTheme }),
  z.object({
    type: z.literal("files"),
    diff_token: zToken,
    files: z.array(zGitDiffFile),
    collapsed: z.array(zWirePath),
    view: z.enum(["auto", "split", "unified"]),
    wrap: z.boolean(),
    actions: z.array(zAction),
    drafts: z.array(zReviewComment),
  }),
  z.object({
    type: z.literal("render"),
    diff_token: zToken,
    path: zWirePath,
    hunks: z.array(zHunk),
  }),
  z.object({ type: z.literal("scroll_to"), path: zWirePath, line: z.number().int().optional() }),
  z.object({ type: z.literal("clear") }),
]);

export const zFromReview = z.discriminatedUnion("type", [
  z.object({ type: z.literal("ready"), v: z.literal(REVIEW_PROTOCOL) }),
  z.object({ type: z.literal("need_file"), diff_token: zToken, path: zWirePath }),
  z.object({ type: z.literal("set_collapsed"), path: zWirePath, collapsed: z.boolean() }),
  /** Hunks of one file; without `hunk_ids`, the whole file. */
  z.object({
    type: z.literal("hunk_action"),
    diff_token: zToken,
    action: zAction,
    path: zWirePath,
    hunk_ids: z.array(zHunkId).min(1).max(1000).optional(),
  }),
  z.object({ type: z.literal("draft_comment"), diff_token: zToken, comment: zReviewComment }),
  z.object({
    type: z.literal("key_forward"),
    key: z.string().max(32),
    code: z.string().max(32),
    meta: z.boolean(),
    shift: z.boolean(),
    alt: z.boolean(),
    ctrl: z.boolean(),
  }),
]);

export type ToReview = z.infer<typeof zToReview>;
export type FromReview = z.infer<typeof zFromReview>;
export type ReviewHunk = z.infer<typeof zHunk>;

/** `data` as a message of `schema`, or `undefined` when it does not fit. */
export function parse<T>(schema: z.ZodType<T>, data: unknown): T | undefined {
  const parsed = schema.safeParse(data);
  return parsed.success ? parsed.data : undefined;
}

/**
 * One file's patch for @pierre/diffs, from the units of the host that carry
 * lines (`hunk` and `whole_file`): their text as it is, which pierre shows as
 * text (design 5.7.8 (i)).
 */
export function patchOf(file: GitDiffFile, hunks: readonly ReviewHunk[]): string {
  const from = file.change === "add" ? "/dev/null" : `a/${file.old_path ?? file.path}`;
  const to = file.change === "delete" ? "/dev/null" : `b/${file.path}`;
  const body = hunks
    .filter((hunk) => hunk.unit !== "mode")
    .flatMap(({ header, lines }) => under(header.split("\n").filter(isHead), lines));
  return [`--- ${from}`, `+++ ${to}`, ...body, ""].join("\n");
}

export const isHead = (line: string) => line.startsWith("@@ ");

/** A whole file's lines under its `@@` lines, by the counts each one gives. */
function under(heads: string[], lines: readonly string[]): string[] {
  if (heads.length < 2) return [...heads, ...lines];
  const out: string[] = [];
  let at = 0;
  for (const head of heads) {
    const [, old = "1", now = "1"] = /^@@ -\d+(?:,(\d+))? \+\d+(?:,(\d+))? @@/.exec(head) ?? [];
    let [left, right] = [Number(old), Number(now)];
    out.push(head);
    for (; at < lines.length; at++) {
      const line = lines[at] ?? "";
      const sign = line[0];
      if (left <= 0 && right <= 0 && sign !== "\\") break;
      out.push(line);
      if (sign !== "+" && sign !== "\\") left--;
      if (sign !== "-" && sign !== "\\") right--;
    }
  }
  return out;
}
