// The review document's view (design 5.7.5): pierre's CodeView over the
// files of the diff on screen, as the main document sends them. It holds
// nothing of its own: the files, which are folded, their hunks and where to
// scroll come down the port. CodeView is the scroll container itself and
// renders only the rows near the screen (T104). The headers are ours: the
// fold, the path and the counts, and a line for what pierre does not show.

import type { GitDiffFile } from "@cupper/protocol";
import { getSingularPatch, type ChangeTypes, type FileDiffMetadata } from "@pierre/diffs";
import {
  CodeView,
  WorkerPoolContextProvider,
  type CodeViewDiffItem,
  type CodeViewHandle,
} from "@pierre/diffs/react";
import { useEffect, useMemo, useRef, useSyncExternalStore } from "react";

import { ChevronDownIcon, GitStatusIcon } from "../../components/icons";
import { patchOf, type ReviewHunk, type ToReview } from "../protocol";

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

export interface DocProps {
  state: DocState;
  need(path: string): void;
  setCollapsed(path: string, collapsed: boolean): void;
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

function Diff({ state, files, need, setCollapsed }: DocProps & { files: Files }) {
  const view = useRef<CodeViewHandle<undefined, undefined>>(null);
  const wide = useSyncExternalStore(onResize, () => window.innerWidth >= SPLIT_MIN_WIDTH);
  const split = files.view === "split" || (files.view === "auto" && wide);
  const items = useItems(files, state.hunks);
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
      }) as const,
    [state.theme, split, files.wrap],
  );
  const { scroll } = state;
  useEffect(() => {
    if (!scroll || !byPath.has(scroll.path)) return;
    const { path: id, line } = scroll;
    view.current?.scrollTo(
      line ? { type: "line", id, lineNumber: line } : { type: "item", id, align: "start" },
    );
  }, [scroll, byPath]);
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
              file={file}
              collapsed={item.collapsed === true}
              hunks={state.hunks.get(file.path)}
              strings={state.strings}
              need={need}
              setCollapsed={setCollapsed}
            />
          )
        );
      }}
    />
  );
}

let version = 0;

/** One item per file, made again only when its hunks or its fold changed. */
function useItems(files: Files, hunks: DocState["hunks"]) {
  const made = useRef(new Map<string, { key: unknown[]; item: CodeViewDiffItem }>());
  return useMemo(() => {
    const collapsed = new Set(files.collapsed);
    const next = new Map<string, { key: unknown[]; item: CodeViewDiffItem }>();
    const items = files.files.map((file) => {
      const shown = hunks.get(file.path);
      const key = [files.diff_token, shown, collapsed.has(file.path)];
      const old = made.current.get(file.path);
      const item: CodeViewDiffItem =
        old && key.every((part, i) => part === old.key[i])
          ? old.item
          : {
              id: file.path,
              type: "diff",
              fileDiff: metadata(file, shown, files.diff_token),
              collapsed: collapsed.has(file.path),
              version: ++version,
            };
      next.set(file.path, { key, item });
      return item;
    });
    made.current = next;
    return items;
  }, [files, hunks]);
}

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
  need(path: string): void;
  setCollapsed(path: string, collapsed: boolean): void;
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
