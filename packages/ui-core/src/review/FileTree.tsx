// The review panel's file tree (design 5.7.5): the diff's files under their
// directories, which stick to the top as they scroll; a filter; each file's
// git status as an icon; the actions its scope offers; a right-click menu.
// Choosing a file scrolls the diff view to it.

import type { GitDiffFile } from "@cupper/protocol";
import { useState, type ReactNode } from "react";
import { useStore } from "zustand";

import { useHost, useT } from "../app/providers";
import { FolderIcon, GitStatusIcon, MinusIcon, PlusIcon, UndoIcon } from "../components/icons";
import { ContextMenu, ContextMenuItem } from "../components/Menu";
import { Tooltip } from "../components/Tooltip";
import { useToasts } from "../feedback/toaster";
import { failureText } from "./actions";
import { useReviewStore } from "./state";
import { ACTIONS, type HunkAction, type ReviewViewProps } from "./view";

interface Dir {
  dirs: Map<string, Dir>;
  files: GitDiffFile[];
}

function build(files: readonly GitDiffFile[]): Dir {
  const root: Dir = { dirs: new Map(), files: [] };
  for (const file of files) {
    let dir = root;
    const parts = file.path.split("/");
    for (const name of parts.slice(0, -1)) {
      const next = dir.dirs.get(name) ?? { dirs: new Map(), files: [] };
      dir.dirs.set(name, next);
      dir = next;
    }
    dir.files.push(file);
  }
  return root;
}

/** A directory that holds one directory and nothing else shares its row: "src/review". */
function chain(name: string, dir: Dir): [string, Dir] {
  const only = dir.files.length === 0 && dir.dirs.size === 1 ? [...dir.dirs][0] : undefined;
  return only ? chain(`${name}/${only[0]}`, only[1]) : [name, dir];
}

const ROW = 28;
const ICONS: Record<HunkAction, ReactNode> = {
  stage: <PlusIcon />,
  unstage: <MinusIcon />,
  revert: <UndoIcon />,
};
const COLOR: Record<string, string> = { add: "text-diff-add", delete: "text-diff-del" };

/** A long name loses its middle: the end, where the extension is, stays. Read whole. */
function Name({ text }: { text: string }) {
  const cut = text.length - Math.min(10, Math.floor(text.length / 2));
  return (
    <>
      <span className="sr-only">{text}</span>
      <span aria-hidden className="flex min-w-0">
        <span className="truncate">{text.slice(0, cut)}</span>
        <span className="shrink-0">{text.slice(cut)}</span>
      </span>
    </>
  );
}

type TreeProps = Pick<ReviewViewProps, "sessionId" | "scope" | "diff" | "act">;

export function FileTree(props: TreeProps) {
  const t = useT();
  const [filter, setFilter] = useState("");
  const query = filter.trim().toLowerCase();
  const files = props.diff.files.filter((file) => file.path.toLowerCase().includes(query));
  return (
    <nav
      aria-label={t("review:files")}
      className="flex w-60 shrink-0 flex-col border-r border-border"
    >
      <div className="px-2 pb-2">
        <input
          type="text"
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
          placeholder={t("review:filter_files")}
          aria-label={t("review:filter_files")}
          className="h-7 w-full rounded-md bg-btn-secondary px-2 text-sm outline-none placeholder:text-fg-tertiary focus-visible:ring-2 focus-visible:ring-ring"
        />
      </div>
      {files.length === 0 ? (
        <p className="px-3 text-sm text-fg-tertiary">{t("review:no_match")}</p>
      ) : (
        <ul className="min-h-0 flex-1 overflow-y-auto px-1 pb-18 text-sm">
          <Rows {...props} dir={build(files)} depth={0} />
        </ul>
      )}
    </nav>
  );
}

function Rows(props: TreeProps & { dir: Dir; depth: number }) {
  const { dir, depth } = props;
  const indent = { paddingLeft: 8 + depth * 12 };
  return (
    <>
      {[...dir.dirs].map(([first, sub]) => {
        const [name, node] = chain(first, sub);
        return (
          <li key={name}>
            <div
              style={{ top: depth * ROW, ...indent }}
              className="sticky z-1 flex h-7 items-center gap-1.5 bg-surface text-fg-secondary"
            >
              <FolderIcon />
              <Name text={name} />
            </div>
            <ul>
              <Rows {...props} dir={node} depth={depth + 1} />
            </ul>
          </li>
        );
      })}
      {dir.files.map((file) => (
        <FileRow key={file.path} {...props} file={file} indent={indent} />
      ))}
    </>
  );
}

function FileRow(props: TreeProps & { file: GitDiffFile; indent: { paddingLeft: number } }) {
  const t = useT();
  const { client } = useHost();
  const { show } = useToasts();
  const { focus, set } = useStore(useReviewStore());
  const { file, scope } = props;
  const selected = focus?.path === file.path;
  const name = file.path.slice(file.path.lastIndexOf("/") + 1);
  const open = () =>
    client
      .request("workspace/open", { session_id: props.sessionId, target: "editor" })
      .catch((error: unknown) => {
        const { key, code } = failureText(error);
        show({ id: "review", text: t(key, { code }) });
      });
  const items = (
    <>
      <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(file.path)}>
        {t("review:copy_path")}
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void open()}>{t("review:open_external")}</ContextMenuItem>
    </>
  );
  return (
    <li>
      <ContextMenu items={items}>
        <div
          style={props.indent}
          className={`group flex h-7 items-center rounded-md pr-0.5 hover:bg-hover ${selected ? "bg-accent-soft" : ""}`}
        >
          <button
            type="button"
            title={file.path}
            aria-current={selected || undefined}
            onClick={() => set({ focus: { path: file.path, seq: (focus?.seq ?? 0) + 1 } })}
            className="flex h-full min-w-0 flex-1 items-center gap-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <span className={COLOR[file.change] ?? "text-diff-mod"}>
              <GitStatusIcon change={file.change} />
            </span>
            <Name text={name} />
          </button>
          {ACTIONS[scope].map((action) => (
            <Tooltip key={action} content={t(`review:action_${action}`)}>
              <button
                type="button"
                aria-label={t(`review:action_${action}`)}
                onClick={() => props.act(action, { paths: [file.path] }, file.path)}
                className="flex size-7 shrink-0 items-center justify-center rounded-full text-icon-secondary opacity-0 outline-none group-hover:opacity-100 hover:bg-hover focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring"
              >
                {ICONS[action]}
              </button>
            </Tooltip>
          ))}
        </div>
      </ContextMenu>
    </li>
  );
}
