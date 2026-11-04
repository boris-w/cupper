// Carrying out the review panel's git actions (design 5.7.5, 5.2.10): stage
// and unstage at once, revert only after the confirmation (RP-11). Each one
// names the `diff_token` of the diff it was chosen on; the host refuses a
// token that no longer matches (`stale_diff`), and the panel loads again.

import { RpcFailure } from "@cupper/host-client";
import type { DiffToken, GitTarget } from "@cupper/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";

import { useHost, useT } from "../app/providers";
import { useToasts } from "../feedback/toaster";
import type { HunkAction } from "./view";

export interface Action {
  action: HunkAction;
  diffToken: DiffToken;
  target: GitTarget;
  /** The one path the action is about, for its result. */
  path?: string;
}

/** A revert waiting for its confirmation; `source` sets the focus (RP-11). */
export interface PendingRevert extends Omit<Action, "action"> {
  source: "button" | "view";
  hunk: boolean;
}

/** What the toast says for a failed action, by the error's semantic code. */
export function failureText(error: unknown): { key: string; code?: string } {
  const data = error instanceof RpcFailure ? error.error.data : undefined;
  const { code, reason } = (data ?? {}) as { code?: string; reason?: string };
  switch (reason) {
    case "stale_diff":
      return { key: "review:stale" };
    case "outcome_unknown":
      return { key: "review:outcome_unknown" };
    case "user_activation_required":
      return { key: "review:activation" };
    default:
      return { key: "review:failed", code: reason ?? code ?? "unexpected" };
  }
}

export function useActions(sessionId: string) {
  const { client } = useHost();
  const query = useQueryClient();
  const { show } = useToasts();
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<PendingRevert>();
  // Another session's panel: its revert waits no more (the panel stays mounted).
  const [owner, setOwner] = useState(sessionId);
  if (owner !== sessionId) {
    setOwner(sessionId);
    setPending(undefined);
  }

  const run = useCallback(
    async ({ action, diffToken, target, path }: Action) => {
      setBusy(true);
      try {
        await client.command(`git/${action}`, {
          session_id: sessionId,
          diff_token: diffToken,
          target,
        });
        const done = { stage: "review:staged", unstage: "review:unstaged", revert: "" }[action];
        const text = done
          ? t(done)
          : t(path ? "review:reverted_path" : "review:reverted", { path });
        show({ id: "review", text });
      } catch (error) {
        const { key, code } = failureText(error);
        show({ id: "review", text: t(key, { code }) });
      } finally {
        setBusy(false);
        // Whatever happened, the repository is what counts (GT-1).
        void query.invalidateQueries({ queryKey: ["session", sessionId] });
      }
    },
    [client, query, sessionId, show, t],
  );

  /** One confirmation at a time: another revert asked for while one waits is ignored (RP-11). */
  const askRevert = useCallback(
    (revert: PendingRevert) => setPending((shown) => shown ?? revert),
    [],
  );
  const settle = useCallback(
    (confirmed: boolean) => {
      setPending(undefined);
      if (confirmed && pending) {
        void run({ action: "revert", ...pending });
      }
    },
    [pending, run],
  );
  return { run, busy, pending, askRevert, settle };
}
