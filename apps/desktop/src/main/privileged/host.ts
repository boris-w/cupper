// What the privileged actions ask of the host, on the control connection.

import { SessionView, type HostClient } from "@cupper/host-client";
import type { SessionConfigured } from "@cupper/protocol";

export interface PrivilegedHost {
  request<R>(method: string, params: Record<string, unknown>): Promise<R>;
  command<R>(method: string, params: Record<string, unknown>): Promise<R>;
  /**
   * The session's latest `session.configured`, from one snapshot of its
   * stream (design 5.7.3); `undefined` when the stream fails or does not
   * come in time.
   */
  configured(sessionId: string): Promise<SessionConfigured | undefined>;
}

/** On `client`; `delay` resolves after its milliseconds. */
export function controlHost(
  client: HostClient,
  delay: (ms: number) => Promise<void>,
  limitMs = 5000,
): PrivilegedHost {
  return {
    request: (method, params) => client.request(method, params),
    command: (method, params) => client.command(method, params),
    configured: (sessionId) => {
      const view = new SessionView();
      let off: () => void = () => undefined;
      const snapshot = new Promise<SessionConfigured | undefined>((resolve) => {
        off = client.subscribe(`session/${sessionId}`, {
          reset: () => view.reset(),
          apply: (body, event) => view.apply(body, event),
          live: () => resolve(view.toJSON().configured ?? undefined),
          failed: () => resolve(undefined),
        });
      });
      const late = delay(limitMs).then(() => undefined);
      return Promise.race([snapshot, late]).finally(() => off());
    },
  };
}
