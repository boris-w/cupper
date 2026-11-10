// Sending from the composer (design 5.7.4): while a turn runs, a message
// steers it or queues behind it as `sessionSend` decides (`mode: queue`
// starts a turn when none runs). On the new session page, the first message
// creates the session. Text that would not fit the request in one frame goes
// up as a blob first and is sent as a `text_blob` (design 5.3.4).

import { ConnectionLost, MAX_FRAME_BYTES, RpcFailure } from "@cupper/host-client";
import type { PromptBlock, SessionSendResult } from "@cupper/protocol";
import { useCallback, useEffect, useState } from "react";

import { useHost, useShell, useT } from "../app/providers";
import { useToasts } from "../feedback/toaster";
import { useStreams, useUi } from "../state/provider";
import { useEntry } from "../state/stream";
import { FOLLOW_UP_QUEUE_PREF, steers, type SendAction } from "./keys";

/** The draft key of the new session page. */
export const NEW_DRAFT = "new";

/** What the envelope and `command_id` add to a request's params, with room to spare. */
const ENVELOPE_BYTES = 1_024;

const utf8 = (text: string) => new TextEncoder().encode(text);

export function codeOf(error: unknown): string {
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

/** What a new session is created with (design 5.7.4); fixed once it exists. */
export interface NewSession {
  agent: string;
  workspace: { kind: string };
}

export const DEFAULT_NEW_SESSION: NewSession = { agent: "claude", workspace: { kind: "local" } };

/**
 * Sends the user's message to session `sessionId` (design 5.7.4, RP-12):
 * steering the turn the session view shows running when `action` asks to
 * (`steers`) and the agent can, else queued. A steer the agent cannot take
 * says "已转为排队" once in the session; the first steer of an agent with
 * side effects says what they are, once. A steer the host queued instead
 * (`fallback_reason`) says "已转为排队" each time. While a steer waits on
 * the host, the session counts it (`steering`): an answer may be long in
 * coming, and the composer does not wait for it.
 */
export function useSessionSend(sessionId: string) {
  const { client } = useHost();
  const streams = useStreams();
  const ui = useUi();
  const t = useT();
  const { show } = useToasts();
  const once = useHintOnce(sessionId);
  const followUpQueue = usePref(FOLLOW_UP_QUEUE_PREF) === true;
  return useCallback(
    async (content: PromptBlock[], action: SendAction, more: { reply_to?: string } = {}) => {
      const view = streams.session(sessionId)?.view.toJSON();
      const turn = view?.turns.findLast((turn) => !turn.completed)?.started.turn_id;
      const can = view?.capabilities?.turn.steer;
      const wanted = steers(action, turn !== undefined, followUpQueue);
      const steer = wanted && can?.supported === true;
      if (wanted && !steer) once("queued_instead");
      if (steer && can.side_effects) once("steer_side_effects");
      const mode = steer ? { mode: "steer", expected_turn_id: turn } : { mode: "queue" };
      ui.getState().markSent(sessionId);
      if (steer) ui.getState().addSteering(sessionId, 1);
      try {
        const sent = await client.command<SessionSendResult>("session/send", {
          ...{ session_id: sessionId, content, ...more, ...mode },
        });
        if (sent.fallback_reason)
          show({ id: "queued_instead", text: t("composer:queued_instead") });
      } finally {
        if (steer) ui.getState().addSteering(sessionId, -1);
      }
    },
    [client, streams, ui, t, show, once, sessionId, followUpQueue],
  );
}

/** Shows hint `composer:{name}` the first time it comes in session `sessionId`. */
export function useHintOnce(sessionId: string) {
  const ui = useUi();
  const t = useT();
  const { show } = useToasts();
  return useCallback(
    (name: string) => {
      if (ui.getState().hint(sessionId, name)) show({ id: name, text: t(`composer:${name}`) });
    },
    [ui, show, t, sessionId],
  );
}

export function useSend(
  sessionId: string | undefined,
  projectId: string | undefined,
  create: NewSession = DEFAULT_NEW_SESSION,
) {
  const { client, handshake } = useHost();
  const budget = (handshake?.limits.max_frame_bytes ?? MAX_FRAME_BYTES) - ENVELOPE_BYTES;
  const ui = useUi();
  const t = useT();
  const { show } = useToasts();
  const fail = useCallback(
    (error: unknown) =>
      show({ id: "send", text: t("composer:send_failed", { code: codeOf(error) }) }),
    [show, t],
  );
  const key = sessionId ?? NEW_DRAFT;
  const toSession = useSessionSend(key);

  /**
   * Clears the draft at once, and puts it back if the message did not go and
   * nothing was typed since. Resolves to whether the message went.
   */
  const send = useCallback(
    async (text: string, attachments: readonly PromptBlock[], action: SendAction = "send") => {
      const content: PromptBlock[] = [
        ...(text.trim() ? [{ type: "text" as const, text }] : []),
        ...attachments,
      ];
      const state = ui.getState();
      state.setDraft(key, "");
      state.setAttachments(key, []);
      const params = (blocks: PromptBlock[]) =>
        sessionId !== undefined
          ? { session_id: sessionId, mode: "queue", content: blocks }
          : { ...create, project_id: projectId, message: blocks };
      try {
        let fitted = content;
        if (utf8(JSON.stringify(params(content))).length > budget && text.trim()) {
          const blob = await client.putBlob(utf8(text), "text/plain");
          fitted = [{ type: "text_blob", blob }, ...attachments];
        }
        if (sessionId !== undefined) {
          await toSession(fitted, action);
        } else {
          const created = await client.command<{ session_id: string }>(
            "session/create",
            params(fitted),
          );
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
    [client, ui, key, sessionId, projectId, fail, budget, create, toSession],
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
