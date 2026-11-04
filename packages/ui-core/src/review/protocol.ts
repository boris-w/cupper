// The review document's port protocol (design 5.7.5), version 1: what the
// main document and the review document say to each other over the
// MessageChannel the main document made. Both sides parse every message
// with these schemas and drop one that does not fit. The main document
// holds every state; the review document is a view of it.
//
// Down: `init`, `theme_changed`, `locale_changed`; `files` (the diff's files,
// what is folded, the drafts and the actions of the scope); `render` (one
// file's hunks, the answer to `need_file`); `scroll_to`; `clear`.
// Up: `ready`, `need_file`, `set_collapsed`, `hunk_action`, `draft_comment`,
// `key_forward`.

import { zGitDiffFile, zHunkId, zReviewComment, zWirePath } from "@cupper/protocol";
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
  z.object({ type: z.literal("locale_changed"), locale: z.string().max(35), strings: zStrings }),
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
 * One file's unified patch for @pierre/diffs, from the host's hunks: the
 * text of the lines as it is, which pierre shows as text (design 5.7.8 (i)).
 */
export function patchOf(path: string, oldPath: string | undefined, hunks: readonly ReviewHunk[]) {
  const body = hunks
    .filter((hunk) => hunk.header.startsWith("@@"))
    .flatMap((hunk) => [hunk.header, ...hunk.lines]);
  return [`--- a/${oldPath ?? path}`, `+++ b/${path}`, ...body, ""].join("\n");
}
