// Privileged actions (design 5.7.8) and what the user sees of their outcome.
// A confirmed action shows in the UI through the streams, and the user
// cancelled the dialog themselves; every other outcome gets a toast.

import type { PrivilegedAction, PrivilegedResult } from "@cupper/shell-api";
import { useCallback } from "react";

import { useShell, useT } from "../app/providers";
import { useToasts } from "./toaster";

export interface Feedback {
  key: string;
  values?: Record<string, unknown>;
}

export function feedbackOf(result: PrivilegedResult): Feedback | undefined {
  switch (result.status) {
    case "busy":
      return { key: "shell:privileged_busy" };
    case "cooldown":
      return {
        key: "shell:privileged_cooldown",
        values: { seconds: Math.ceil(result.retry_after_ms / 1000) },
      };
    case "user_activation_required":
      return { key: "shell:privileged_activation" };
    case "failed":
      return { key: "shell:privileged_failed", values: { code: codeOf(result.error) } };
    default:
      return undefined;
  }
}

/** The semantic code the main process gives a failure (`{code, reason?}`). */
function codeOf(error: unknown): string {
  const code = typeof error === "object" && error !== null && "code" in error ? error.code : null;
  return typeof code === "string" ? code : "unexpected";
}

/** Runs a privileged action through the shell, and tells the user when it did not go through. */
export function usePrivileged() {
  const { api } = useShell();
  const t = useT();
  const { show } = useToasts();
  return useCallback(
    async (action: PrivilegedAction): Promise<PrivilegedResult> => {
      const result = await api
        .requestPrivileged(action)
        .catch((): PrivilegedResult => ({ status: "failed", error: { code: "unexpected" } }));
      const feedback = feedbackOf(result);
      if (feedback) {
        show({ id: `privileged:${action.kind}`, text: t(feedback.key, feedback.values) });
      }
      return result;
    },
    [api, t, show],
  );
}
