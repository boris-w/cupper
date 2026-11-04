// The "变更" tab of the side panel (design 5.7.5, GT-1 to GT-3, Codex's style,
// ui-spec 4.6): the header with the scope, the counts and the options; the
// caveats and the notice of a changed repository configuration; the file
// tree (T232) beside the diff view, or an empty state; the bulk actions'
// pill; and the revert confirmation. The Git actions (T233) join later. The
// diff view, the review document, stays loaded from one session to the next
// and while there is nothing to show.

import { RpcFailure } from "@cupper/host-client";
import type { GitDiffResult } from "@cupper/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { use, useMemo, useState, type ReactNode } from "react";
import { useStore } from "zustand";

import { useHost, useT } from "../app/providers";
import { SlotsContext } from "../app/slots";
import { Alert } from "../components/Alert";
import { ChevronDownIcon, MoreIcon, PaneIcon, Spinner } from "../components/icons";
import { Menu, MenuItem, MenuLabel, MenuSeparator } from "../components/Menu";
import { ToolbarButton } from "../components/ToolbarButton";
import { useToasts } from "../feedback/toaster";
import { useUi } from "../state/provider";
import { useActions } from "./actions";
import { FileTree } from "./FileTree";
import { GitActions } from "./GitActions";
import { ReviewFrame } from "./ReviewFrame";
import { RevertConfirm } from "./RevertConfirm";
import {
  createReviewStore,
  diffKey,
  ReviewContext,
  SCOPES,
  useDiff,
  useReviewStore,
  useStatus,
  type DiffView,
} from "./state";
import { ACTIONS, addComment, commentsOf, type ReviewViewProps } from "./view";

interface PanelProps {
  /** The file tree's preference (⌘⇧E), which the shell keeps. */
  fileTree: boolean;
  onFileTree(): void;
  /** Whether the panel is on screen; a hidden one asks the host for nothing. */
  active: boolean;
}

export function ReviewPanel(props: PanelProps) {
  const ui = useUi();
  const sessionId = useStore(ui, ({ route }) =>
    route.page === "session" ? route.sessionId : undefined,
  );
  const [store] = useState(createReviewStore);
  return (
    <ReviewContext value={store}>
      <Review sessionId={sessionId} {...props} />
    </ReviewContext>
  );
}

function Review(props: PanelProps & { sessionId?: string }) {
  const sessionId = props.sessionId ?? "";
  // A hidden panel, or one with no session, asks the host for nothing.
  const enabled = props.active && props.sessionId !== undefined;
  const t = useT();
  const ui = useUi();
  const store = useReviewStore();
  const state = useStore(store);
  const diff = useDiff(sessionId, state.scope, enabled);
  const status = useStatus(sessionId, enabled);
  const actions = useActions(sessionId);
  const Slot = use(SlotsContext).reviewView;
  const attachments = useStore(ui, (ui) => ui.attachments[sessionId]);
  const drafts = useMemo(() => commentsOf(attachments), [attachments]);
  const changed = status.data?.repo_config_changed ?? [];
  const files = diff.data?.files ?? [];
  // The diff view's and the file tree's actions, within what the scope offers.
  const act: ReviewViewProps["act"] = (action, target, label) => {
    if (!diff.data || !ACTIONS[state.scope].includes(action)) {
      return;
    }
    const diffToken = diff.data.diff_token;
    if (action === "revert") {
      const hunk = "hunk_ids" in target;
      actions.askRevert({ source: "view", diffToken, target, path: label, hunk });
    } else {
      void actions.run({ action, diffToken, target, path: label });
    }
  };

  let body: ReactNode;
  let view: ReviewViewProps | undefined;
  if (!props.sessionId) {
    body = <Empty title={t("review:no_session")} />;
  } else if (diff.isPending) {
    body = <Spinner className="m-auto" />;
  } else if (diff.isError) {
    const data = diff.error instanceof RpcFailure ? diff.error.error.data : undefined;
    body =
      (data as { reason?: string } | undefined)?.reason === "not_a_repo" ? (
        <Empty title={t("review:not_a_repo")} hint={t("review:not_a_repo_hint")} />
      ) : (
        <Empty title={t("review:load_failed")}>
          <button type="button" className={PILL_BUTTON} onClick={() => void diff.refetch()}>
            {t("review:retry")}
          </button>
        </Empty>
      );
  } else if (files.length === 0) {
    const scope = state.scope;
    body = <Empty title={t(`review:empty_${scope}`)} hint={t(`review:empty_${scope}_hint`)} />;
  } else {
    view = {
      sessionId,
      scope: state.scope,
      diff: diff.data,
      wrap: state.wrap,
      view: state.view,
      collapsed: state.collapsed,
      focus: state.focus,
      drafts,
      setCollapsed: (path, folded) => {
        const collapsed = new Set(state.collapsed);
        if (folded) collapsed.add(path);
        else collapsed.delete(path);
        state.set({ collapsed });
      },
      act,
      comment: (draft) => addComment(ui, sessionId, draft),
    };
  }
  const shown = view && diff.data;

  return (
    <div className="relative flex h-full flex-col">
      {props.sessionId && (
        <Header
          diff={diff.data}
          refresh={() => void diff.refetch()}
          {...props}
          sessionId={sessionId}
        />
      )}
      {(changed.length > 0 || (diff.data?.caveats.length ?? 0) > 0) && (
        <div className="flex flex-col gap-2 px-3 pb-2 [&>*]:max-w-none">
          {changed.length > 0 && (
            <Alert>{t("review:config_changed", { paths: changed.join("、") })}</Alert>
          )}
          {diff.data?.caveats.map((code) => (
            <Alert key={code}>
              {["mixed_changes", "late_tool_effects"].includes(code)
                ? t(`review:caveat_${code}`)
                : t("review:caveat_other", { code })}
            </Alert>
          ))}
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        {props.fileTree && shown && (
          <FileTree
            key={sessionId}
            sessionId={sessionId}
            scope={state.scope}
            diff={shown}
            act={act}
          />
        )}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {Slot ? view && <Slot {...view} /> : <ReviewFrame view={view} />}
          {body}
        </div>
      </div>
      {shown && <BulkActions sessionId={sessionId} diff={shown} actions={actions} />}
      {actions.pending && <RevertConfirm pending={actions.pending} settle={actions.settle} />}
    </div>
  );
}

/** "查阅来源", the counts and the options menu (design 5.7.5). */
function Header(props: {
  sessionId: string;
  diff?: GitDiffResult;
  refresh(): void;
  fileTree: boolean;
  onFileTree(): void;
  active: boolean;
}) {
  const { diff, refresh } = props;
  const t = useT();
  const state = useStore(useReviewStore());
  const views: DiffView[] = ["auto", "split", "unified"];
  return (
    <div className="flex h-(--layout-pane-header-h) shrink-0 items-center gap-2 px-2">
      <Menu
        align="start"
        trigger={
          <ToolbarButton label={t("review:source")}>
            <span className="text-fg">{t(`review:scope_${state.scope}`)}</span>
            <ChevronDownIcon />
          </ToolbarButton>
        }
      >
        <MenuLabel>{t("review:source")}</MenuLabel>
        {SCOPES.map((scope) => (
          <MenuItem
            key={scope}
            radio
            checked={scope === state.scope}
            onSelect={() => state.set({ scope })}
          >
            {t(`review:scope_${scope}`)}
          </MenuItem>
        ))}
      </Menu>
      {diff && diff.stat.files > 0 && (
        <span className="text-sm tabular-nums">
          <span className="text-diff-add">+{diff.stat.added}</span>{" "}
          <span className="text-diff-del">−{diff.stat.removed}</span>
        </span>
      )}
      <div className="flex-1" />
      <GitActions sessionId={props.sessionId} active={props.active} />
      <ToolbarButton
        label={t(props.fileTree ? "review:hide_files" : "review:show_files")}
        shortcut="⇧⌘E"
        pressed={props.fileTree}
        onClick={props.onFileTree}
      >
        <PaneIcon side="left" />
      </ToolbarButton>
      <Menu
        trigger={
          <ToolbarButton label={t("review:options")}>
            <MoreIcon />
          </ToolbarButton>
        }
      >
        <MenuItem checked={state.wrap} onSelect={() => state.set({ wrap: !state.wrap })}>
          {t("review:wrap")}
        </MenuItem>
        <MenuSeparator />
        {views.map((view) => (
          <MenuItem
            key={view}
            radio
            checked={view === state.view}
            onSelect={() => state.set({ view })}
          >
            {t(`review:view_${view}`)}
          </MenuItem>
        ))}
        <MenuSeparator />
        <MenuItem onSelect={() => state.set({ collapsed: new Set() })}>
          {t("review:expand_all")}
        </MenuItem>
        <MenuItem
          onSelect={() => state.set({ collapsed: new Set(diff?.files.map((f) => f.path)) })}
        >
          {t("review:collapse_all")}
        </MenuItem>
        <MenuSeparator />
        <MenuItem onSelect={refresh}>{t("review:refresh")}</MenuItem>
      </Menu>
    </div>
  );
}

const PILL_BUTTON =
  "h-7 rounded-full px-3 text-sm outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";

/**
 * The pill floating 20 above the panel's bottom (design 5.7.5): "还原全部"
 * and "暂存全部", or "取消暂存全部" alone in the staged scope. A turn's
 * changes are staged by their paths, on the unstaged diff, the one the host
 * stages from.
 */
function BulkActions(props: {
  sessionId: string;
  diff: GitDiffResult;
  actions: ReturnType<typeof useActions>;
}) {
  const t = useT();
  const { client } = useHost();
  const query = useQueryClient();
  const { show } = useToasts();
  const { scope } = useStore(useReviewStore());
  const { sessionId, diff, actions } = props;
  const diffToken = diff.diff_token;

  const stageAll = async () => {
    if (scope !== "last_turn") {
      return actions.run({ action: "stage", diffToken, target: { all: true } });
    }
    const unstaged = await query.fetchQuery({
      queryKey: diffKey(sessionId, "unstaged"),
      queryFn: () =>
        client.request<GitDiffResult>("git/diff", { session_id: sessionId, scope: "unstaged" }),
    });
    const turn = new Set(diff.files.map((file) => file.path));
    const paths = unstaged.files.map((file) => file.path).filter((path) => turn.has(path));
    if (paths.length === 0) {
      show({ id: "review", text: t("review:nothing_to_stage") });
      return;
    }
    return actions.run({ action: "stage", diffToken: unstaged.diff_token, target: { paths } });
  };
  const revertAll = () =>
    actions.askRevert({ source: "button", diffToken, target: { all: true }, hunk: false });

  return (
    <div className="absolute inset-x-0 bottom-5 flex justify-center">
      <div className="flex gap-1 rounded-full bg-surface-elevated p-1 shadow-pill">
        {scope === "staged" ? (
          <PillButton
            disabled={actions.busy}
            onClick={() =>
              void actions.run({ action: "unstage", diffToken, target: { all: true } })
            }
          >
            {t("review:unstage_all")}
          </PillButton>
        ) : (
          <>
            <PillButton disabled={actions.busy} onClick={revertAll}>
              {t("review:revert_all")}
            </PillButton>
            <PillButton disabled={actions.busy} onClick={() => void stageAll()}>
              {t("review:stage_all")}
            </PillButton>
          </>
        )}
      </div>
    </div>
  );
}

function PillButton(props: { disabled: boolean; onClick(): void; children: ReactNode }) {
  return (
    <button type="button" disabled={props.disabled} onClick={props.onClick} className={PILL_BUTTON}>
      {props.children}
    </button>
  );
}

function Empty(props: { title: string; hint?: string; children?: ReactNode }) {
  return (
    <div className="m-auto flex max-w-64 flex-col items-center gap-1 px-4 text-center">
      <p className="text-sm">{props.title}</p>
      {props.hint && <p className="text-sm text-fg-tertiary">{props.hint}</p>}
      {props.children && <div className="mt-2">{props.children}</div>}
    </div>
  );
}
