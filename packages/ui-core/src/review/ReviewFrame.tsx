// The diff view as the review document (design 5.7.5), its main document
// side: an iframe on the review document's own origin, sandboxed, and the
// MessageChannel made for it; only that channel's messages count. The
// panel's props go down as `files`, `render` and `scroll_to`. What comes up
// is checked against the diff on screen and the hunks this side fetched,
// then becomes the panel's `act` and `comment`; nothing the review document
// says is carried out as it is, and a revert only opens the confirmation.
// The frame stays loaded while there is no diff to show, hidden.

import type { GitFileDiffResult, ReviewComment } from "@cupper/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";

import { useHost, useShell, useT } from "../app/providers";
import {
  parse,
  REVIEW_PROTOCOL,
  zFromReview,
  type FromReview,
  type ReviewHunk,
  type ToReview,
} from "./protocol";
import { ACTIONS, type ReviewViewProps } from "./view";

/** The review document's words, which come from the main document's language. */
const STRINGS = ["doc_binary", "doc_mode", "doc_fold", "doc_unfold", "doc_loading"];

/** A file's hunks in the diff of `diffToken` (T130's query key). */
export const fileDiffKey = (sessionId: string, diffToken: string, path: string) =>
  ["session", sessionId, "git/file_diff", diffToken, path] as const;

export function ReviewFrame({ view }: { view?: ReviewViewProps }) {
  const { api, info } = useShell();
  const { client } = useHost();
  const query = useQueryClient();
  const t = useT();
  const frame = useRef<HTMLIFrameElement>(null);
  const port = useRef<MessagePort | undefined>(undefined);
  const latest = useRef(view);
  latest.current = view;
  /** The hunk IDs of each file this side fetched, in the diff on screen. */
  const fetched = useRef(new Map<string, ReadonlySet<string>>());
  const send = (message: ToReview) => port.current?.postMessage(message);

  const hunksOf = async ({ sessionId, diff }: ReviewViewProps, path: string) => {
    const token = diff.diff_token;
    const result = await query.fetchQuery({
      queryKey: fileDiffKey(sessionId, token, path),
      queryFn: () =>
        client.request<GitFileDiffResult>("git/file_diff", {
          session_id: sessionId,
          diff_token: token,
          path,
        }),
    });
    const text = new TextDecoder();
    const hunks: ReviewHunk[] = await Promise.all(
      result.hunks.map(async ({ hunk_id, unit, header, lines, lines_blob }) => {
        const blob = lines_blob && text.decode(await client.getBlob(lines_blob));
        return { hunk_id, unit, header, lines: lines ?? blob?.split("\n").slice(0, -1) ?? [] };
      }),
    );
    if (latest.current?.diff.diff_token === token) {
      fetched.current.set(path, new Set(hunks.map((hunk) => hunk.hunk_id)));
    }
    return hunks;
  };

  const receive = (message: FromReview) => {
    const current = latest.current;
    if (message.type === "ready") return sendFiles();
    if (message.type === "key_forward") {
      const { key, code, meta, shift, alt, ctrl } = message;
      const init = { key, code, metaKey: meta, shiftKey: shift, altKey: alt, ctrlKey: ctrl };
      const event = new KeyboardEvent("keydown", { ...init, bubbles: true, cancelable: true });
      return void frame.current?.dispatchEvent(event);
    }
    if (!current) return;
    const { diff, scope } = current;
    const path = message.type === "draft_comment" ? message.comment.path : message.path;
    const file = diff.files.find((f) => f.path === path);
    if (!file) return;
    switch (message.type) {
      case "set_collapsed":
        return current.setCollapsed(file.path, message.collapsed);
      case "need_file":
        if (message.diff_token !== diff.diff_token) return;
        // A file whose hunks fail to come stays loading until the diff is new.
        return void hunksOf(current, file.path).then(
          (hunks) => send({ type: "render", diff_token: diff.diff_token, path: file.path, hunks }),
          () => undefined,
        );
      case "hunk_action": {
        const known = fetched.current.get(file.path);
        const ids = message.hunk_ids;
        const ok =
          message.diff_token === diff.diff_token &&
          ACTIONS[scope].includes(message.action) &&
          (ids === undefined || ids.every((id) => known?.has(id)));
        if (ok)
          current.act(message.action, ids ? { hunk_ids: ids } : { paths: [file.path] }, file.path);
        return;
      }
      case "draft_comment": {
        const draft: ReviewComment = message.comment;
        const lines = draft.line_start <= draft.line_end;
        if (message.diff_token === diff.diff_token && lines && draft.comment.trim()) {
          current.comment(draft);
        }
      }
    }
  };

  const sendFiles = () => {
    if (!latest.current) return send({ type: "clear" });
    const { diff, collapsed, view, wrap } = latest.current;
    const files = { diff_token: diff.diff_token, files: diff.files, collapsed: [...collapsed] };
    send({ type: "files", ...files, view, wrap });
  };

  // A channel for each load of the review document, which takes the first port only.
  const onLoad = () => {
    const target = frame.current?.contentWindow;
    const { protocol, host } = new URL(info.reviewDocumentUrl);
    if (!target || !host) return;
    port.current?.close();
    const { port1, port2 } = new MessageChannel();
    port.current = port1;
    port1.onmessage = ({ data }) => {
      const message = parse(zFromReview, data);
      if (message) receive(message);
    };
    const theme = document.documentElement.dataset.theme === "dark" ? "dark" : "light";
    const strings = Object.fromEntries(STRINGS.map((key) => [key, t(`review:${key}`)]));
    const init = { type: "init", v: REVIEW_PROTOCOL, theme, locale: info.locale, strings };
    target.postMessage(init, `${protocol}//${host}`, [port2]);
  };

  useEffect(
    () => api.system.onThemeChange((theme) => send({ type: "theme_changed", theme })),
    [api],
  );
  useEffect(() => () => port.current?.close(), []);
  const { diff, collapsed, view: shown, wrap, focus } = view ?? {};
  useEffect(() => fetched.current.clear(), [diff?.diff_token]);
  // Again whenever what `files` carries changed.
  useEffect(sendFiles, [diff, collapsed, shown, wrap]);
  useEffect(() => {
    if (focus) send({ type: "scroll_to", path: focus.path, line: focus.line });
  }, [focus]);

  return (
    <iframe
      ref={frame}
      title={t("review:doc_title")}
      src={info.reviewDocumentUrl}
      sandbox="allow-scripts allow-same-origin"
      allow="clipboard-write"
      onLoad={onLoad}
      hidden={!view}
      className="block min-h-0 w-full flex-1 border-0"
    />
  );
}
