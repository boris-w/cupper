// Answering a request (design 5.3.6, 5.7.8): `interaction/respond`, whose
// real-input evidence the main process adds, or the native dialog for a
// privileged option.

import { RpcFailure } from "@cupper/host-client";
import type { InteractionOutcome, InteractionRequested } from "@cupper/protocol";
import { useCallback } from "react";

import { useHost, useShell, useT } from "../app/providers";
import { useToasts } from "../feedback/toaster";

/** Rule 7: the main process ignores keys for this long after the window comes forward. */
export const COOLDOWN_MS = 1000;

/**
 * How an answer went: `sent`; `again` when it lacked real input within the
 * cooldown after the window came forward, the one case for "请再按一次";
 * `revoke` when it lacked real input otherwise (say, right after typing), so
 * the panel leaves keyboard activation (RP-3); `elsewhere` when another
 * client answered first or the request expired; `failed` otherwise.
 */
export type Answered = "sent" | "again" | "revoke" | "elsewhere" | "failed";

export function useRespond(sessionId: string) {
  const { client } = useHost();
  const { focusedAt } = useShell();
  const t = useT();
  const { show } = useToasts();
  return useCallback(
    async (requested: InteractionRequested, outcome: InteractionOutcome): Promise<Answered> => {
      try {
        await client.command("interaction/respond", {
          session_id: sessionId,
          request_id: requested.request_id,
          request_digest: requested.digest,
          outcome,
        });
        return "sent";
      } catch (error) {
        const data = error instanceof RpcFailure ? error.error.data : undefined;
        const { code, reason } = (data ?? {}) as { code?: string; reason?: string };
        if (reason === "user_activation_required") {
          return Date.now() - focusedAt.current < COOLDOWN_MS ? "again" : "revoke";
        }
        const elsewhere = reason === "already_resolved" || code === "expired";
        const text = elsewhere
          ? t("request:elsewhere")
          : t("request:failed", { code: reason ?? code ?? "unexpected" });
        show({ id: "respond", text });
        return elsewhere ? "elsewhere" : "failed";
      }
    },
    [client, sessionId, show, t, focusedAt],
  );
}
