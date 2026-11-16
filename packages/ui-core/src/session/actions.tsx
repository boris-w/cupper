// The session actions (design 5.7.4, 5.5.9; UI-0): rename, archive with the
// worktree's cleanup, and open the workspace in another app. The sidebar, the
// title bar and the app menu's commands all come here; one provider in the
// shell runs them and shows their dialogs. Titles are agent or user text, only
// ever text.

import { RpcFailure } from "@cupper/host-client";
import { escapeInvisible } from "@cupper/i18n";
import type {
  ArchiveWorktree,
  SessionArchiveResult,
  SessionSummary,
  WorktreeCheckResult,
} from "@cupper/protocol";
import { commandHandler } from "@cupper/shell-api";
import { createContext, use, useEffect, useMemo, useState, type ReactNode } from "react";

import { required, useHost, useShell, useT, type Translate } from "../app/providers";
import { useToasts } from "../feedback/toaster";
import { useStreams, useUi } from "../state/provider";
import { ArchiveDialog, blockerText, RenameDialog } from "./dialogs";

export type OpenTarget = "terminal" | "editor" | "finder";

export interface SessionActions {
  /** Asks for the new title in a dialog. */
  rename(sessionId: string): void;
  /** A worktree session asks first whether its worktree goes too. */
  archive(sessionId: string): void;
  open(sessionId: string, target: OpenTarget): void;
}

const Actions = createContext<SessionActions | null>(null);

export const useSessionActions = () => required(use(Actions), "SessionActionsProvider");

/** The reason, else the code, of a failed call. */
export function failureCode(error: unknown): string {
  const data = error instanceof RpcFailure ? error.error.data : undefined;
  const { code, reason } = (data ?? {}) as { code?: string; reason?: string };
  return reason ?? code ?? "unexpected";
}

export function failureText(t: Translate, error: unknown): string {
  const code = failureCode(error);
  return code === "turn_running" ? t("session:turn_running") : t("session:failed", { code });
}

export const titleOf = (t: Translate, session: SessionSummary) =>
  escapeInvisible(session.title) || t("session:untitled");

type Pending =
  | { kind: "rename"; session: SessionSummary }
  | { kind: "archive"; session: SessionSummary; check: WorktreeCheckResult };

export function SessionActionsProvider({ children }: { children: ReactNode }) {
  const t = useT();
  const { api } = useShell();
  const { client } = useHost();
  const { show } = useToasts();
  const ui = useUi();
  const { view } = useStreams().index;
  const [pending, setPending] = useState<Pending>();

  const run = useMemo(() => {
    const toast = (text: string) => show({ id: "session", text });
    const fail = (error: unknown) => toast(failureText(t, error));
    const rename = async (session: SessionSummary, title: string) => {
      await client.command("session/rename", { session_id: session.session_id, title });
    };
    // Once archived, the window leaves it.
    const archive = async (session: SessionSummary, worktree: ArchiveWorktree) => {
      const params = { session_id: session.session_id, worktree };
      const result = await client.command<SessionArchiveResult>("session/archive", params);
      const { route, navigate } = ui.getState();
      if (route.page === "session" && route.sessionId === session.session_id) {
        navigate({ page: "new" });
      }
      const title = titleOf(t, session);
      const reasons = (result.blockers ?? []).map((code) => blockerText(t, code)).join("、");
      const outcome = session.workspace_kind === "worktree" ? `_${result.worktree}` : "";
      toast(t(`session:archived${outcome}`, { title, reasons }));
    };
    const actions: SessionActions = {
      rename: (id) => {
        const session = view.toJSON().sessions[id];
        if (session) setPending({ kind: "rename", session });
      },
      archive: (id) => {
        const session = view.toJSON().sessions[id];
        if (session?.workspace_kind === "worktree") {
          client
            .request<WorktreeCheckResult>("worktree/check", { session_id: id })
            .then((check) => setPending({ kind: "archive", session, check }), fail);
        } else if (session) {
          archive(session, "keep").catch(fail);
        }
      },
      open: (id, target) => {
        client.request("workspace/open", { session_id: id, target }).catch(fail);
      },
    };
    return { actions, rename, archive, fail };
  }, [client, show, t, ui, view]);

  // The app menu's commands, on the session the window shows.
  useEffect(() => {
    const on = (act: (id: string) => void) => () => {
      const { route } = ui.getState();
      if (route.page === "session") act(route.sessionId);
    };
    const { actions } = run;
    return api.onCommand(
      commandHandler({
        rename: on(actions.rename),
        archive: on(actions.archive),
        open_in_terminal: on((id) => actions.open(id, "terminal")),
      }),
    );
  }, [api, run, ui]);

  const close = () => setPending(undefined);
  // The dialog stays while its call runs; a failure says why in a toast.
  const settle = (call: Promise<void>) => call.catch(run.fail).finally(close);
  return (
    <Actions value={run.actions}>
      {children}
      {pending?.kind === "rename" && (
        <RenameDialog
          title={pending.session.title}
          onClose={close}
          onRename={(title) => settle(run.rename(pending.session, title))}
        />
      )}
      {pending?.kind === "archive" && (
        <ArchiveDialog
          check={pending.check}
          onClose={close}
          onArchive={(worktree) => settle(run.archive(pending.session, worktree))}
        />
      )}
    </Actions>
  );
}
