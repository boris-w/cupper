import type { ClientState } from "@cupper/host-client";
import { useEffect } from "react";

import { useT } from "../app/providers";
import { useToasts } from "./toaster";

/**
 * The host connection while it is down (design 5.7.1): "reconnecting" or
 * "failed" stays up until the connection is ready again.
 */
export function ConnectionToast({ state }: { state: ClientState }) {
  const t = useT();
  const { show, dismiss } = useToasts();
  const down = state.status === "reconnecting" || state.status === "failed";
  useEffect(() => {
    if (down) {
      show({ id: "connection", text: t(`shell:host_${state.status}`), sticky: true });
    } else {
      dismiss("connection");
    }
  }, [down, state.status, t, show, dismiss]);
  return null;
}
