// The diff view as the review document (design 5.7.5): its main document
// side. An iframe on the review document's own origin, sandboxed, and the
// MessageChannel made for it; only that channel's messages count. The
// panel's props go down as `files`, `render` and `scroll_to`. What comes up
// is checked against the diff on screen and the hunks this side fetched,
// then becomes the panel's `act` and `comment`; nothing the review document
// says is carried out as it is, and a revert only opens the confirmation.

import type { GitFileDiffResult, ReviewComment } from "@cupper/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";

import { useHost, useShell, useT } from "../app/providers";
import { parse, REVIEW_PROTOCOL, zFromReview, type FromReview, type ReviewHunk, type ToReview } from "./protocol";
import { ACTIONS, type ReviewViewProps } from "./view";

/** The words the review document shows, from the main document's language. */
const STRINGS = ["doc_binary", "doc_mode", "doc_fold", "doc_unfold", "doc_loading"];

/** The review document's `file_diff` of `path`, as the review panel keys it (T130). */
export const fileDiffKey = (sessionId: string, diffToken: string, path: string) =>
  ["session", sessionId, "git/file_diff", diffToken, path] as const;

export function ReviewFrame(props: ReviewViewProps) {
  const { api, info } = useShell();
  const { client } = useHost();
  const query = useQueryClient();
  const t = useT();
  const frame = useRef<HTMLIFrameElement>(null);
  const port = useRef<MessagePort | undefined>(undefined);
  const latest = useRef(props);
  latest.current = props;
  /** The hunks each file had when this side fetched it, for the diff on screen. */
  const fetched = useRef(new Map<string, Set<string>>());
  const send = (message: ToReview) => port.current?.postMessage(message);
  const strings = () => Object.fromEntries(STRINGS.map((key) => [key, t(`review:${key}`)]));

  const hunksOf = async (path: string): Promise<ReviewHunk[]> => {
    const { sessionId, diff } = latest.current;
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
    const hunks = await Promise.all(
      result.hunks.map(async ({ hunk_id, unit, header, lines, lines_blob }) => {
        const blob = lines_blob && text.decode(await client.getBlob(lines_blob));
        return { hunk_id, unit, header, lines: lines ?? blob?.split("\n").slice(0, -1) ?? [] };
      }),
    );
    if (latest.current.diff.diff_token === token) {
      fetched.current.set(path, new Set(hunks.map((hunk) => hunk.hunk_id)));
    }
    return hunks;
  };

  const receive = (message: FromReview) => {
    const { diff, scope, act, comment, setCollapsed } = latest.current;
    const file = "path" in message ? diff.files.find((f) => f.path === message.path) : undefined;
    const current = "diff_token" in message && message.diff_token === diff.diff_token;
    switch (message.type) {
      case "ready":
        return sendFiles();
      case "need_file":
        if (current && file) {
          const path = file.path;
          void hunksOf(path).then((hunks) =>
            send({ type: "render", diff_token: message.diff_token, path, hunks }),
          );
        }
        return;
      case "set_collapsed":
        return file && setCollapsed(file.path, message.collapsed);
      case "hunk_action": {
        const known = fetched.current.get(message.path);
        const ids = message.hunk_ids;
        const ok =
          current && file && ACTIONS[scope].includes(message.action) &&
          (ids === undefined || ids.every((id) => known?.has(id)));
        if (ok) act(message.action, ids ? { hunk_ids: ids } : { paths: [file.path] }, file.path);
        return;
      }
      case "draft_comment": {
        const draft: ReviewComment = message.comment;
        const ok = current && diff.files.some((f) => f.path === draft.path) && draft.comment.trim();
        if (ok) comment(draft);
        return;
      }
      case "key_forward": {
        const { key, code, meta, shift, alt, ctrl } = message;
        const init = { key, code, metaKey: meta, shiftKey: shift, altKey: alt, ctrlKey: ctrl };
        document.dispatchEvent(new KeyboardEvent("keydown", { ...init, bubbles: true }));
      }
    }
  };

  const sendFiles = () => {
    const { diff, scope, collapsed, view, wrap, drafts } = latest.current;
    send({
      type: "files",
      diff_token: diff.diff_token,
      files: diff.files,
      collapsed: [...collapsed],
      view,
      wrap,
      actions: [...ACTIONS[scope]],
      drafts: [...drafts],
    });
  };

  // The channel, once the review document loaded: it takes the first port only.
  const onLoad = () => {
    const target = frame.current?.contentWindow;
    if (!target) return;
    port.current?.close();
    const { port1, port2 } = new MessageChannel();
    port.current = port1;
    port1.onmessage = ({ data }) => {
      const message = parse(zFromReview, data);
      if (message) receive(message);
    };
    const theme = document.documentElement.dataset.theme === "dark" ? "dark" : "light";
    const init = { v: REVIEW_PROTOCOL, theme, locale: document.documentElement.lang, strings: strings() } as const;
    target.postMessage({ type: "init", ...init }, new URL(info.reviewDocumentUrl).origin, [port2]);
  };

  useEffect(
    () => api.system.onThemeChange((theme) => send({ type: "theme_changed", theme })),
    [api],
  );
  const { diff, collapsed, view, wrap, drafts, scope, focus } = props;
  useEffect(() => {
    fetched.current.clear();
  }, [diff.diff_token]);
  // biome-ignore: the props the review document shows.
  useEffect(sendFiles, [diff, collapsed, view, wrap, drafts, scope]);
  useEffect(() => {
    if (focus) send({ type: "scroll_to", path: focus.path, ...(focus.line ? { line: focus.line } : {}) });
  }, [focus]);
  useEffect(() => () => port.current?.close(), []);

  return (
    <iframe
      ref={frame}
      title={t("review:doc_title")}
      src={info.reviewDocumentUrl}
      sandbox="allow-scripts allow-same-origin"
      allow="clipboard-write"
      onLoad={onLoad}
      className="block min-h-0 w-full flex-1 border-0"
    />
  );
}
