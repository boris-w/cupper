// The review document's view (design 5.7.5): pierre's CodeView over the
// files of the diff on screen, as the main document sends them. It holds
// nothing of its own: the files, which are folded, their hunks and where to
// scroll come down the port. CodeView is the scroll container itself and
// renders only the rows near the screen (T104). The headers are ours: the
// fold, the path, the counts and the file's actions, and a line for what
// pierre does not show. Each hunk ends with its actions; the "+" pierre shows
// on a hovered line number opens a comment box under the lines it took, and
// the drafts show under theirs (T238). Both only ask the main document.

import type { GitDiffFile, ReviewComment } from "@cupper/protocol";
import {
  getSingularPatch,
  type AnnotationSide,
  type ChangeTypes,
  type DiffLineAnnotation,
  type FileDiffMetadata,
  type SelectedLineRange,
} from "@pierre/diffs";
import {
  CodeView,
  WorkerPoolContextProvider,
  type CodeViewDiffItem,
  type CodeViewHandle,
} from "@pierre/diffs/react";
import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from "react";

import {
  ChevronDownIcon,
  GitStatusIcon,
  MinusIcon,
  PlusIcon,
  UndoIcon,
} from "../../components/icons";
import { isHead, patchOf, type ReviewHunk, type ToReview } from "../protocol";

type Files = Extract<ToReview, { type: "files" }>;

export interface DocState {
  theme: "light" | "dark";
  strings: Readonly<Record<string, string>>;
  files?: Files;
  /** The hunks that came for the files of `files.diff_token`. */
  hunks: ReadonlyMap<string, readonly ReviewHunk[]>;
  /** A new `seq` repeats a jump to the same place. */
  scroll?: { path: string; line?: number; seq: number };
}

type Action = Files["actions"][number];

export interface DocProps {
  state: DocState;
  need(path: string): void;
  setCollapsed(path: string, collapsed: boolean): void;
  /** An action on a file's hunks, or on the whole file without `hunkIds`. */
  act(action: Action, path: string, hunkIds?: string[]): void;
  comment(draft: ReviewComment): void;
}

/** What a row under a line holds: a hunk's actions, a draft, or the comment box. */
type Note =
  | { kind: "hunk"; ids: string[] }
  | { kind: "draft"; draft: ReviewComment }
  | { kind: "box"; range: SelectedLineRange };
type Item = CodeViewDiffItem<Note>;
/** The lines a comment box is open under. */
interface Box {
  path: string;
  range: SelectedLineRange;
}

/** From this width on, the `auto` view shows the diff split; below it, unified. */
export const SPLIT_MIN_WIDTH = 800;

const POOL = {
  workerFactory: () => new Worker(new URL("./worker.ts", import.meta.url), { type: "module" }),
  poolSize: 2,
};
// No language up front: each loads when a file of it is first highlighted.
const HIGHLIGHTER = { preferredHighlighter: "shiki-js" as const, langs: [] };

/** The pool and its Worker start with the document, before the first diff (design 5.7.5). */
export function ReviewDocument(props: DocProps) {
  const { files } = props.state;
  return (
    <WorkerPoolContextProvider poolOptions={POOL} highlighterOptions={HIGHLIGHTER}>
      {files && <Diff {...props} files={files} />}
    </WorkerPoolContextProvider>
  );
}

const onResize = (changed: () => void) => {
  window.addEventListener("resize", changed);
  return () => window.removeEventListener("resize", changed);
};

function Diff({ state, files, ...props }: DocProps & { files: Files }) {
  const view = useRef<CodeViewHandle<Note, undefined>>(null);
  const [box, setBox] = useState<Box>();
  const wide = useSyncExternalStore(onResize, () => window.innerWidth >= SPLIT_MIN_WIDTH);
  const split = files.view === "split" || (files.view === "auto" && wide);
  const items = useItems(files, state.hunks, box);
  const byPath = useMemo(() => new Map(files.files.map((file) => [file.path, file])), [files]);
  const options = useMemo(
    () =>
      ({
        themeType: state.theme,
        diffStyle: split ? "split" : "unified",
        overflow: files.wrap ? "wrap" : "scroll",
        preferredHighlighter: "shiki-js",
        // A plain line between hunks: pierre's own words are not translated.
        hunkSeparators: "simple",
        stickyHeaders: true,
        itemMetrics: { diffHeaderHeight: 36 },
        // Room under the last file for the panel's pill.
        layout: { paddingTop: 0, paddingBottom: 72, gap: 0 },
        // The "+" on a hovered line number; a drag from it takes more lines.
        enableGutterUtility: true,
        onGutterUtilityClick: (range: SelectedLineRange, { item }: { item: { id: string } }) =>
          setBox({ path: item.id, range }),
      }) as const,
    [state.theme, split, files.wrap],
  );
  // The lines the box took stay marked while it is open.
  const close = () => {
    setBox(undefined);
    view.current?.clearSelectedLines();
  };
  const { scroll } = state;
  useEffect(() => {
    if (!scroll || !byPath.has(scroll.path)) return;
    const { path: id, line } = scroll;
    view.current?.scrollTo(
      line ? { type: "line", id, lineNumber: line } : { type: "item", id, align: "start" },
    );
  }, [scroll, byPath]);
  const { strings } = state;
  return (
    <CodeView
      ref={view}
      items={items}
      options={options}
      className="h-screen overflow-auto"
      renderCustomHeader={(item) => {
        const file = byPath.get(item.id);
        return (
          file && (
            <Header
              {...props}
              file={file}
              collapsed={item.collapsed === true}
              hunks={state.hunks.get(file.path)}
              strings={strings}
              actions={files.actions}
            />
          )
        );
      }}
      renderAnnotation={({ metadata: note }, source, { id: path }) => {
        if (note.kind === "hunk") {
          const act = (action: Action) => props.act(action, path, note.ids);
          return (
            <div className="flex justify-end gap-1 px-2 py-1">
              <Actions actions={files.actions} strings={strings} act={act} />
            </div>
          );
        }
        if (note.kind === "draft") {
          return <p className={`${CARD} whitespace-pre-wrap`}>{note.draft.comment}</p>;
        }
        if (!("hunks" in source)) return null;
        const save = (text: string) => {
          props.comment(draftOf(source, path, note.range, text));
          close();
        };
        return <CommentBox strings={strings} save={save} close={close} />;
      }}
    />
  );
}

const CARD = "mx-3 my-1.5 rounded-xl border border-border bg-surface-elevated px-3 py-2 text-sm";

let version = 0;

/** One item per file, made again only when its hunks, its fold or its rows changed. */
function useItems(files: Files, hunks: DocState["hunks"], box?: Box) {
  const made = useRef(new Map<string, { key: unknown[]; item: Item }>());
  return useMemo(() => {
    const collapsed = new Set(files.collapsed);
    const next = new Map<string, { key: unknown[]; item: Item }>();
    const items = files.files.map((file) => {
      const shown = hunks.get(file.path);
      const drafts = files.drafts.filter((draft) => draft.path === file.path);
      const open = box?.path === file.path ? box.range : undefined;
      const rows = JSON.stringify([files.actions.length > 0, drafts, open]);
      const key = [files.diff_token, shown, collapsed.has(file.path), rows];
      const old = made.current.get(file.path);
      let item = old?.item;
      if (!item || !key.every((part, i) => part === old?.key[i])) {
        const fileDiff = metadata(file, shown, files.diff_token);
        const annotations = notes(fileDiff, shown, files.actions.length > 0, drafts, open);
        const fold = collapsed.has(file.path);
        item = {
          id: file.path,
          type: "diff",
          fileDiff,
          annotations,
          collapsed: fold,
          version: ++version,
        };
      }
      next.set(file.path, { key, item });
      return item;
    });
    made.current = next;
    return items;
  }, [files, hunks, box]);
}

/**
 * The rows under a file's lines: each unit's actions under its last hunk,
 * when the scope has any, then the drafts and the open comment box.
 */
function notes(
  fileDiff: FileDiffMetadata,
  hunks: readonly ReviewHunk[] = [],
  actions: boolean,
  drafts: readonly ReviewComment[],
  box?: SelectedLineRange,
): DiffLineAnnotation<Note>[] {
  // Pierre's hunks in order, by the host's unit each one is part of.
  const ids = hunks
    .filter((hunk) => hunk.unit !== "mode" && hunk.lines.length > 0)
    .flatMap((hunk) =>
      hunk.header
        .split("\n")
        .filter(isHead)
        .map(() => hunk.hunk_id),
    );
  const ends: DiffLineAnnotation<Note>[] = actions
    ? fileDiff.hunks.flatMap((hunk, i) => {
        const id = ids[i];
        if (!id || ids[i + 1] === id) return [];
        const added = hunk.additionCount > 0;
        const lineNumber = added
          ? hunk.additionStart + hunk.additionCount - 1
          : hunk.deletionStart + hunk.deletionCount - 1;
        const side: AnnotationSide = added ? "additions" : "deletions";
        return [{ side, lineNumber, metadata: { kind: "hunk" as const, ids: [id] } }];
      })
    : [];
  const shown: DiffLineAnnotation<Note>[] = drafts.map((draft) => ({
    side: draft.side === "old" ? "deletions" : "additions",
    lineNumber: draft.line_end,
    metadata: { kind: "draft", draft },
  }));
  if (box) {
    const side = box.endSide ?? box.side ?? "additions";
    shown.push({ side, lineNumber: box.end, metadata: { kind: "box", range: box } });
  }
  return [...ends, ...shown];
}

/** A draft of the lines `range` took, with their text as the quote. */
function draftOf(
  fileDiff: FileDiffMetadata,
  path: string,
  range: SelectedLineRange,
  comment: string,
) {
  const side = range.side ?? "additions";
  const added = side === "additions";
  const [first, last] = [Math.min(range.start, range.end), Math.max(range.start, range.end)];
  const quote: string[] = [];
  for (let n = first; n <= last; n++) {
    const hunk = fileDiff.hunks.find((h) =>
      added
        ? n >= h.additionStart && n < h.additionStart + h.additionCount
        : n >= h.deletionStart && n < h.deletionStart + h.deletionCount,
    );
    const at =
      hunk &&
      (added
        ? hunk.additionLineIndex + n - hunk.additionStart
        : hunk.deletionLineIndex + n - hunk.deletionStart);
    const line =
      at === undefined ? undefined : (added ? fileDiff.additionLines : fileDiff.deletionLines)[at];
    if (line !== undefined) quote.push(line.replace(/\r?\n$/, ""));
  }
  const lines = { line_start: first, line_end: last, side: added ? "new" : "old" };
  return { path, ...lines, quote: quote.join("\n"), comment };
}

const ICONS: Record<Action, ReactNode> = {
  stage: <PlusIcon />,
  unstage: <MinusIcon />,
  revert: <UndoIcon />,
};

/** The scope's actions as icon buttons (design 5.7.5): ghost, round, 28 across, named in the tooltip. */
function Actions(props: {
  actions: readonly Action[];
  strings: DocState["strings"];
  act(action: Action): void;
}) {
  return props.actions.map((action) => (
    <button
      key={action}
      type="button"
      title={props.strings[`action_${action}`]}
      aria-label={props.strings[`action_${action}`]}
      onClick={() => props.act(action)}
      className="flex size-7 items-center justify-center rounded-full text-icon-secondary outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring"
    >
      {ICONS[action]}
    </button>
  ));
}

/** The comment box: a draft only. Esc closes it, ⌘Enter keeps it; its keys stay here. */
function CommentBox(props: {
  strings: DocState["strings"];
  save(text: string): void;
  close(): void;
}) {
  const [text, setText] = useState("");
  const save = () => text.trim() && props.save(text);
  return (
    <div className={CARD}>
      <textarea
        autoFocus
        value={text}
        aria-label={props.strings.doc_comment}
        placeholder={props.strings.doc_comment}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") props.close();
          if (event.key === "Enter" && event.metaKey) save();
        }}
        className="block h-16 w-full resize-none bg-transparent outline-none placeholder:text-fg-tertiary"
      />
      <div className="mt-2 flex justify-end gap-2">
        <button
          type="button"
          onClick={props.close}
          className={`${BUTTON} bg-btn-secondary hover:bg-hover`}
        >
          {props.strings.cancel}
        </button>
        <button
          type="button"
          disabled={!text.trim()}
          onClick={save}
          className={`${BUTTON} bg-btn-primary text-btn-primary-fg disabled:opacity-50`}
        >
          {props.strings.doc_comment_save}
        </button>
      </div>
    </div>
  );
}

const BUTTON = "h-7 rounded-full px-3 outline-none focus-visible:ring-2 focus-visible:ring-ring";

const TYPES: Readonly<Record<string, ChangeTypes>> = { add: "new", delete: "deleted" };

/** Nothing to show yet, or nothing pierre shows: a header alone. */
const EMPTY: FileDiffMetadata = {
  ...{ name: "", type: "change", hunks: [], isPartial: true },
  ...{ splitLineCount: 0, unifiedLineCount: 0, deletionLines: [], additionLines: [] },
  ...{ deletionCharacterCount: 0, additionCharacterCount: 0 },
};

/**
 * The file's diff for pierre; its names and kind are the host's, not parsed
 * back. Only a diff with lines has a cache key: pierre keeps a layout by it.
 */
function metadata(file: GitDiffFile, hunks: readonly ReviewHunk[] | undefined, token: string) {
  const lines = hunks?.some((hunk) => hunk.unit !== "mode" && hunk.lines.length > 0);
  const type = TYPES[file.change] ?? (file.old_path ? "rename-changed" : "change");
  const names = { name: file.path, prevName: file.old_path, type };
  if (!lines || !hunks) return { ...EMPTY, ...names };
  return { ...getSingularPatch(patchOf(file, hunks)), ...names, cacheKey: `${token}:${file.path}` };
}

function Header(props: {
  file: GitDiffFile;
  collapsed: boolean;
  hunks?: readonly ReviewHunk[];
  strings: DocState["strings"];
  actions: readonly Action[];
  need(path: string): void;
  setCollapsed(path: string, collapsed: boolean): void;
  act(action: Action, path: string): void;
}) {
  const { file, collapsed, hunks, strings, need } = props;
  const loading = !hunks && !file.binary;
  // The header renders once the file nears the screen: its hunks come then.
  useEffect(() => {
    if (loading && !collapsed) need(file.path);
  }, [loading, collapsed, need, file.path]);
  const mode = hunks?.find((hunk) => hunk.unit === "mode")?.header.split("\n");
  const [from, to] = (mode ?? []).map((line) => line.split(" ").at(-1));
  const note = file.binary ? strings.doc_binary : loading ? strings.doc_loading : undefined;
  return (
    <div className="bg-surface">
      <div className="flex h-9 items-center gap-1.5 px-2 text-sm">
        <button
          type="button"
          aria-expanded={!collapsed}
          aria-label={strings[collapsed ? "doc_unfold" : "doc_fold"]}
          onClick={() => props.setCollapsed(file.path, !collapsed)}
          className="flex size-6 items-center justify-center rounded-md outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring"
        >
          <span className={collapsed ? "flex -rotate-90" : "flex"}>
            <ChevronDownIcon />
          </span>
        </button>
        <GitStatusIcon change={file.change} />
        <span className="min-w-0 flex-1 truncate" title={file.path}>
          {file.old_path && <span className="text-fg-tertiary">{file.old_path} → </span>}
          {file.path}
        </span>
        <Actions
          actions={props.actions}
          strings={strings}
          act={(action) => props.act(action, file.path)}
        />
        <span className="text-diff-add tabular-nums">+{file.additions}</span>
        <span className="text-diff-del tabular-nums">−{file.deletions}</span>
      </div>
      {!collapsed && (note || from) && (
        <p className="px-3 pb-2 text-sm text-fg-tertiary">
          {note ?? (
            <>
              {strings.doc_mode} {from} → {to}
            </>
          )}
        </p>
      )}
    </div>
  );
}
