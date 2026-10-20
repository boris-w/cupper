// Sending from the composer (design 5.7.4): M1a has no steer, so a message
// sent while a turn runs is queued (`mode: queue` starts a turn when none
// runs). On the new session page, the first message creates the session.

import { ConnectionLost, RpcFailure } from "@cupper/host-client";
import type { PromptBlock } from "@cupper/protocol";
import { useCallback, useEffect, useState } from "react";

import { useHost, useShell, useT } from "../app/providers";
import { useToasts } from "../feedback/toaster";
import { useStreams, useUi } from "../state/provider";
import { useEntry } from "../state/stream";

/** The draft key of the new session page. */
export const NEW_DRAFT = "new";

function codeOf(error: unknown): string {
  if (error instanceof RpcFailure) {
    const code = (error.error.data as { code?: unknown } | undefined)?.code;
    return typeof code === "string" ? code : String(error.error.code);
  }
  return error instanceof ConnectionLost ? "connection_lost" : "unexpected";
}

/** The session's agent and whether a turn runs, from the index; the new page's defaults. */
export function useSessionState(sessionId?: string) {
  const { view } = useStreams().index;
  useEntry(view, "view");
  const { sessions, projects } = view.toJSON();
  const summary = sessionId === undefined ? undefined : sessions[sessionId];
  const status = summary?.status;
  const first = Object.values(projects).toSorted((a, b) => a.position - b.position)[0];
  return {
    agent: summary?.agent ?? "claude",
    running: status === "running" || status === "needs_action",
    firstProject: first?.project_id,
  };
}

export function useSend(sessionId: string | undefined, projectId: string | undefined) {
  const { client } = useHost();
  const ui = useUi();
  const t = useT();
  const { show } = useToasts();
  const fail = useCallback(
    (error: unknown) =>
      show({ id: "send", text: t("composer:send_failed", { code: codeOf(error) }) }),
    [show, t],
  );
  const key = sessionId ?? NEW_DRAFT;

  /**
   * Clears the draft at once, and puts it back if the message did not go and
   * nothing was typed since. Resolves to whether the message went.
   */
  const send = useCallback(
    async (text: string, attachments: readonly PromptBlock[]) => {
      const content: PromptBlock[] = [
        ...(text.trim() ? [{ type: "text" as const, text }] : []),
        ...attachments,
      ];
      const state = ui.getState();
      state.setDraft(key, "");
      state.setAttachments(key, []);
      try {
        if (sessionId !== undefined) {
          await client.command("session/send", { session_id: sessionId, mode: "queue", content });
        } else {
          const workspace = { kind: "local" };
          const params = { agent: "claude", project_id: projectId, workspace, message: content };
          const created = await client.command<{ session_id: string }>("session/create", params);
          ui.getState().navigate({ page: "session", sessionId: created.session_id });
        }
        return true;
      } catch (error) {
        if (!ui.getState().drafts[key]) {
          ui.getState().setDraft(key, text);
          ui.getState().setAttachments(key, attachments);
        }
        fail(error);
        return false;
      }
    },
    [client, ui, key, sessionId, projectId, fail],
  );

  const command = useCallback(
    (method: string) => void client.command(method, { session_id: sessionId }).catch(fail),
    [client, sessionId, fail],
  );
  return {
    send,
    interrupt: () => command("session/interrupt"),
    resume: () => command("session/queue_resume"),
  };
}

/** A UI preference from the shell, following later changes. */
export function usePref(key: string): unknown {
  const { api, info } = useShell();
  const [value, setValue] = useState(() => info.prefs[key]);
  useEffect(
    () => api.prefs.onChange((changed, next) => changed === key && setValue(next)),
    [api, key],
  );
  return value;
}
