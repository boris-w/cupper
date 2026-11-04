// A finished turn's change card (design 5.7.4, GT-3, F3): "{n} 个文件已更改
// +a −b" by the end snapshot, a line per file of the turn's diff, each opening
// the review panel on it, and "撤销", which reverts the turn after the main
// document's confirmation (RP-11). A snapshot that is not whole says why,
// and offers no undo. Paths are text.

import { RpcFailure } from "@cupper/host-client";
import { escapeInvisible } from "@cupper/i18n";
import { use, useState } from "react";

import { useT } from "../app/providers";
import { Alert } from "../components/Alert";
import { useActions } from "../review/actions";
import { RevertConfirm } from "../review/RevertConfirm";
import { OpenReview, useDiff } from "../review/state";
import { useStreamActions } from "../stream/actions";
import { registerRow, type RowOf } from "./stream";

const REASONS = ["timeout", "unsafe_repo_config", "git_failed"];

function TurnChangesRow({ row }: { row: RowOf<"turn_changes"> }) {
  const t = useT();
  const { sessionId = "" } = useStreamActions();
  const openReview = use(OpenReview);
  const [failure, setFailure] = useState<{ reason?: string; code?: string }>();
  const actions = useActions(sessionId, (error) => {
    const data = error instanceof RpcFailure ? error.error.data : undefined;
    setFailure((data ?? {}) as { reason?: string; code?: string });
  });
  const { status, stat, reason, excluded } = row.snapshot;
  const recorded = status === "ok" || status === "partial";
  const diff = useDiff(sessionId, { turn: row.turnId }, recorded);
  const open = (path?: string) =>
    openReview({ sessionId, turnId: row.turnId, turnNo: row.turnNo, ...(path ? { path } : {}) });
  const undo = () => {
    if (!diff.data) return;
    setFailure(undefined);
    // From a button of the main document: the focus goes to "确认" (RP-11).
    const diffToken = diff.data.diff_token;
    actions.askRevert({ source: "button", diffToken, target: { all: true }, hunk: false });
  };

  return (
    <div
      className="rounded-lg border border-border text-sm hover:border-border-strong"
      data-turn-changes
    >
      <div className="flex items-center gap-2 px-3 py-2">
        {recorded ? (
          <span className="min-w-0 flex-1">
            {t("stream:changes_title", { count: stat?.files ?? 0 })}{" "}
            <span className="text-diff-add">+{stat?.added ?? 0}</span>{" "}
            <span className="text-diff-del">−{stat?.removed ?? 0}</span>
          </span>
        ) : (
          <span className="min-w-0 flex-1">{t("stream:changes_unrecorded")}</span>
        )}
        {status === "ok" && (
          <button
            type="button"
            disabled={!diff.data || actions.busy}
            onClick={undo}
            className="shrink-0 rounded-md px-2 py-0.5 text-fg-secondary outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
          >
            {t("stream:changes_undo")}
          </button>
        )}
      </div>
      {status === "partial" && (
        <p className="px-3 pb-2 text-fg-tertiary">
          {t("stream:changes_partial", { count: excluded.length })}
        </p>
      )}
      {!recorded && (
        <p className="px-3 pb-2 text-fg-tertiary">
          {REASONS.includes(reason ?? "")
            ? t(`stream:changes_reason_${reason}`)
            : t("stream:changes_reason_other", { reason: reason ?? status })}
        </p>
      )}
      {diff.data && diff.data.files.length > 0 && (
        <ul className="border-t border-border py-1">
          {diff.data.files.map((file) => (
            <li key={file.path}>
              <button
                type="button"
                onClick={() => open(file.path)}
                className="flex w-full items-center gap-2 px-3 py-1 text-left outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring"
              >
                <span className="min-w-0 flex-1 truncate">{escapeInvisible(file.path)}</span>
                <span className="text-diff-add">+{file.additions}</span>
                <span className="text-diff-del">−{file.deletions}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
      {failure && (
        <div className="px-3 pb-2 [&_[role]]:max-w-none">
          <Alert
            tone="danger"
            action={
              failure.reason === "stale_diff" && (
                <button type="button" onClick={() => open()} className="shrink-0 text-fg-secondary">
                  {t("stream:changes_open_review")}
                </button>
              )
            }
          >
            {failure.reason === "stale_diff"
              ? t("stream:changes_stale")
              : failure.reason === "lfs_object_missing"
                ? t("stream:changes_lfs")
                : t("review:failed", { code: failure.reason ?? failure.code ?? "unexpected" })}
          </Alert>
        </div>
      )}
      {actions.pending && (
        <div className="px-3 pb-3">
          <RevertConfirm inline pending={actions.pending} settle={actions.settle} />
        </div>
      )}
    </div>
  );
}

registerRow("turn_changes", TurnChangesRow);
