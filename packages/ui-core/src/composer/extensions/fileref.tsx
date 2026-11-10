// File references (design 5.7.4, CP-2): `@` in the text area opens a list of
// the workspace's files from `files/search`, the matched characters bold by
// their UTF-16 ranges; ↑↓ pick, Enter or Tab takes one, Esc closes. A file
// taken, or dropped on the composer, becomes a `file_ref` attachment, its
// path relative to the workspace; a dropped image is `image.tsx`'s. A dropped
// file's path comes from the shell (`pathForFile`); one outside the
// workspace is refused with a hint.

import type { FileMatch, FilesSearchResult, PromptBlock } from "@cupper/protocol";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { use, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useStore } from "zustand";

import { useHost, useShell, useT } from "../../app/providers";
import { useRouter } from "../../app/router";
import { useToasts } from "../../feedback/toaster";
import { useStreams, useUi } from "../../state/provider";
import { OverlayContext, type ComposerExtension } from "../extensions";
import { composing } from "../keys";
import { NEW_DRAFT, useSessionState } from "../send";
import { isImage } from "./image";

const LIMIT = 20;

/** The `@` word the caret ends, from its `@`: where it starts and what follows. */
export function wordAt(text: string, caret: number) {
  const found = /(?:^|\s)@([^\s@]*)$/.exec(text.slice(0, caret));
  return found && { start: caret - (found[1]?.length ?? 0) - 1, query: found[1] ?? "" };
}

/** `abs` relative to the directory `root`, if inside it. */
export function within(root: string, abs: string) {
  const base = root.endsWith("/") ? root : `${root}/`;
  return abs.startsWith(base) && abs.length > base.length ? abs.slice(base.length) : undefined;
}

function FileRefs({ sessionId }: { sessionId?: string }) {
  const t = useT();
  const ui = useUi();
  const { api } = useShell();
  const { client } = useHost();
  const { show } = useToasts();
  const streams = useStreams();
  const { route } = useRouter();
  const { area, container, setClose } = use(OverlayContext);
  const key = sessionId ?? NEW_DRAFT;
  const draft = useStore(ui, (state) => state.drafts[key] ?? "");
  const { firstProject } = useSessionState(sessionId);
  const { sessions, projects } = streams.index.view.toJSON();
  const projectId = sessionId
    ? sessions[sessionId]?.project_id
    : (route.page === "new" && route.projectId) || firstProject;
  const project = projectId === undefined ? undefined : projects[projectId];
  const [caret, setCaret] = useState(0);
  const [closedAt, setClosedAt] = useState<number>();
  const [selected, setSelected] = useState(0);
  const word = wordAt(draft, caret);
  const open = word !== null && word.start !== closedAt && (sessionId ?? projectId) !== undefined;
  const target = sessionId ? { session_id: sessionId } : { project_id: projectId };
  const search = useQuery({
    queryKey: ["files/search", sessionId ?? projectId, word?.query],
    queryFn: () =>
      client.request<FilesSearchResult>("files/search", {
        ...target,
        query: word?.query ?? "",
        limit: LIMIT,
      }),
    enabled: open,
    placeholderData: keepPreviousData,
  });
  const results = (open && search.data?.results) || [];

  const attach = (paths: string[]) => {
    const now = ui.getState().attachments[key] ?? [];
    const have = new Set(now.flatMap((b) => (b.type === "file_ref" ? [b.path] : [])));
    const added: PromptBlock[] = paths
      .filter((path) => !have.has(path))
      .map((path) => ({ type: "file_ref", path }));
    ui.getState().setAttachments(key, [...now, ...added]);
  };
  const back = useRef<number | undefined>(undefined);
  const take = (match: FileMatch) => {
    if (!word) return;
    ui.getState().setDraft(key, draft.slice(0, word.start) + draft.slice(caret));
    back.current = word.start;
    attach([match.path]);
  };
  // The caret goes back where the `@` was, once the draft without the word is shown.
  useLayoutEffect(() => {
    if (back.current === undefined || !area) return;
    area.setSelectionRange(back.current, back.current);
    setCaret(back.current);
    back.current = undefined;
  });

  // The caret, as typing, clicks and keys move it.
  useEffect(() => {
    if (!area) return;
    const follow = () => setCaret(area.selectionStart);
    const events = ["input", "keyup", "click", "focus"] as const;
    events.forEach((name) => area.addEventListener(name, follow));
    return () => events.forEach((name) => area.removeEventListener(name, follow));
  }, [area]);

  useEffect(() => setSelected(0), [word?.query]);
  // An `@` typed again after Esc opens the list again.
  const typing = word !== null;
  useEffect(() => {
    if (!typing) setClosedAt(undefined);
  }, [typing]);
  const latest = useRef({ results, selected, take });
  latest.current = { results, selected, take };
  // While the list is open, its keys come before the composer's.
  useEffect(() => {
    if (!area || !open) return;
    setClose(() => setClosedAt(word?.start));
    const onKey = (event: KeyboardEvent) => {
      // An input method's keys are its own, the list's included.
      if (composing(event)) return;
      const { results, selected, take } = latest.current;
      const move = { ArrowDown: 1, ArrowUp: -1 }[event.key];
      const pick = event.key === "Enter" || event.key === "Tab";
      if ((move === undefined && !pick) || results.length === 0) return;
      event.preventDefault();
      event.stopPropagation();
      if (move !== undefined) {
        setSelected((selected + move + results.length) % results.length);
      } else if (results[selected]) {
        take(results[selected]);
      }
    };
    area.addEventListener("keydown", onKey);
    return () => {
      area.removeEventListener("keydown", onKey);
      setClose(null);
    };
  }, [area, open, word?.start, setClose]);

  // Files dropped on the composer, as the shell gives their paths.
  const root = sessionId
    ? streams.session(sessionId)?.view.toJSON().configured?.workspace.path
    : project?.path;
  useEffect(() => {
    const box = area?.closest("[data-composer]");
    if (!box) return;
    const over = (event: Event) => {
      if ((event as DragEvent).dataTransfer?.types.includes("Files")) event.preventDefault();
    };
    const drop = (event: Event) => {
      // Images are attachments of their own (`image.tsx`).
      const files = [...((event as DragEvent).dataTransfer?.files ?? [])].filter(
        (f) => !isImage(f),
      );
      if (files.length === 0) return;
      event.preventDefault();
      const paths = files.map((file) => {
        const abs = api.pathForFile(file);
        return abs && root ? within(root, abs) : undefined;
      });
      const inside = paths.filter((path) => path !== undefined);
      if (inside.length < paths.length) show({ id: "file_ref", text: t("composer:file_outside") });
      attach(inside);
    };
    box.addEventListener("dragover", over);
    box.addEventListener("drop", drop);
    return () => {
      box.removeEventListener("dragover", over);
      box.removeEventListener("drop", drop);
    };
  });

  if (!open || !container) return null;
  const note = search.isError
    ? t("composer:file_failed")
    : search.isPending
      ? t("composer:file_searching")
      : results.length === 0
        ? t("composer:file_none")
        : undefined;
  return createPortal(
    <div
      role="listbox"
      aria-label={t("composer:file_ref")}
      className="absolute inset-x-0 bottom-2 z-20 max-h-72 overflow-y-auto rounded-2xl bg-surface-elevated p-1 text-sm shadow-card"
    >
      {note && <p className="px-2.5 py-1.5 text-fg-tertiary">{note}</p>}
      {results.map((match, i) => (
        <div
          key={match.path}
          role="option"
          aria-selected={i === selected}
          // The text area keeps the focus.
          onMouseDown={(event) => {
            event.preventDefault();
            take(match);
          }}
          onMouseEnter={() => setSelected(i)}
          className="truncate rounded-lg px-2.5 py-1.5 text-fg-secondary aria-selected:bg-hover"
        >
          <Matched path={match.path} ranges={match.ranges_utf16} />
        </div>
      ))}
      {project?.is_git === false && results.length > 0 && (
        <p className="px-2.5 py-1.5 text-xs text-fg-tertiary">{t("composer:file_walk")}</p>
      )}
    </div>,
    container,
  );
}

/** `path` with its matched UTF-16 ranges in the text colour: slices, never markup. */
function Matched({ path, ranges }: { path: string; ranges: readonly (readonly number[])[] }) {
  const parts: { text: string; hit: boolean }[] = [];
  let at = 0;
  for (const [start = 0, end = 0] of ranges) {
    if (start > at) parts.push({ text: path.slice(at, start), hit: false });
    parts.push({ text: path.slice(start, end), hit: true });
    at = Math.max(at, end);
  }
  parts.push({ text: path.slice(at), hit: false });
  return parts.map((part, i) =>
    part.hit ? (
      <span key={i} className="font-medium text-fg">
        {part.text}
      </span>
    ) : (
      part.text
    ),
  );
}

const extension: ComposerExtension = { side: "left", order: 5, Component: FileRefs };
export default extension;
