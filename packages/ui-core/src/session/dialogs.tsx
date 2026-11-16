// The dialogs of the session actions: the new title, and whether archiving
// takes the worktree too (design 5.5.9). The worktree goes only when the
// host's check found nothing that would be lost; otherwise the dialog lists
// what keeps it, and archiving keeps it. The focus starts on "取消".

import type { ArchiveWorktree, WorktreeCheckResult } from "@cupper/protocol";
import { useState } from "react";

import { useT, type Translate } from "../app/providers";
import { DANGER, Dialog, PRIMARY, SECONDARY } from "../components/Dialog";

/** Why the worktree stays, by the rule of ADR-10 that holds. */
export const blockerText = (t: Translate, code: string) =>
  t(`session:blocker_${code}`, { defaultValue: t("session:blocker_other", { code }) });

export function RenameDialog(props: {
  title: string;
  onClose(): void;
  onRename(title: string): void;
}) {
  const t = useT();
  const [title, setTitle] = useState(props.title);
  const [busy, setBusy] = useState(false);
  const trimmed = title.trim();
  return (
    <Dialog
      title={t("session:rename_title")}
      busy={busy}
      onClose={props.onClose}
      onSubmit={() => {
        if (trimmed === "") return;
        setBusy(true);
        props.onRename(trimmed);
      }}
      actions={
        <button type="submit" disabled={trimmed === ""} className={PRIMARY}>
          {t("session:save")}
        </button>
      }
    >
      <input
        aria-label={t("session:rename_label")}
        value={title}
        autoFocus
        onFocus={(event) => event.target.select()}
        onChange={(event) => setTitle(event.target.value)}
        className="h-8 rounded-lg border border-border bg-surface px-2.5 outline-none focus-visible:ring-2 focus-visible:ring-ring"
      />
    </Dialog>
  );
}

export function ArchiveDialog(props: {
  check: WorktreeCheckResult;
  onClose(): void;
  onArchive(worktree: ArchiveWorktree): void;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const { removable, blockers, warnings } = props.check;
  const archive = (worktree: ArchiveWorktree) => {
    setBusy(true);
    props.onArchive(worktree);
  };
  // Files that look changed only by the case of their paths: the user's word removes it all the same.
  const remove = warnings.case_only_dirty ? "remove_case_only_dirty" : "remove";
  const keep = (label: string, className: string) => (
    <button type="button" onClick={() => archive("keep")} className={className}>
      {label}
    </button>
  );
  return (
    <Dialog
      title={t("session:archive_title")}
      busy={busy}
      onClose={props.onClose}
      actions={
        removable ? (
          <>
            {keep(t("session:archive_keep"), SECONDARY)}
            <button type="button" onClick={() => archive(remove)} className={DANGER}>
              {t("session:archive_remove")}
            </button>
          </>
        ) : (
          keep(t("session:archive"), PRIMARY)
        )
      }
    >
      {removable ? (
        <div className="flex flex-col gap-1 text-fg-secondary">
          <p>{t("session:archive_remove_ask")}</p>
          {warnings.ignored_files > 0 && (
            <p>{t("session:archive_ignored_files", { count: warnings.ignored_files })}</p>
          )}
          {warnings.case_only_dirty && <p>{t("session:archive_case_only")}</p>}
        </div>
      ) : (
        <div className="text-fg-secondary">
          <p>{t("session:archive_blocked")}</p>
          <ul className="mt-1 list-disc ps-5">
            {blockers.map((code) => (
              <li key={code}>{blockerText(t, code)}</li>
            ))}
          </ul>
        </div>
      )}
    </Dialog>
  );
}
