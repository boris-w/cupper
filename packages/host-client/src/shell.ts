import type { HostPort, ShellAPI, Unsubscribe } from "@cupper/shell-api";

import type { HostClient } from "./client";

/**
 * Drives `client` from the renderer: opens this document's data connection
 * and follows its state (design 5.7.1). The main process reconnects, and
 * `ready` brings the new handshake on the same port. Resolves to the
 * unsubscription of the state; `client.close()` closes the port.
 */
export async function connectShell(
  host: ShellAPI["host"],
  client: HostClient,
): Promise<Unsubscribe> {
  let port: HostPort | undefined;
  const off = host.onConnectionState((state, handshake) => {
    if (state.status === "failed") {
      client.fail(state.reason);
    } else if (state.status === "reconnecting") {
      client.detach();
    } else if (state.status !== "ready") {
      client.progress(state.status);
    } else if (port && handshake) {
      client.attach(port, handshake);
    }
  });
  try {
    const opened = await host.open();
    port = opened.port;
    client.attach(port, opened.handshake);
  } catch (err) {
    off();
    throw err;
  }
  return off;
}
