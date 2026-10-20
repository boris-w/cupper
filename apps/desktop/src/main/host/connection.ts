// hostConnection (design 5.7.2): a connection the native extension checked
// (T073), wrapped as a HostPort, then the handshake on it (host-client). Keyed
// builds answer `host/hello` with the extension's `dev_key` proof and check
// `host_proof`; a release build authenticates by signature and refuses a host
// in the degraded mode (`weak_auth`, design 5.8.1).

import { Socket } from "node:net";

import type { HostConnection, KeyedHostConnection } from "@cupper/desktop-native";
import { handshake, HandshakeError, type Handshaken } from "@cupper/host-client";
import type { ClientRole, Feature } from "@cupper/protocol";
import type { ConnectionFailure, HostPort } from "@cupper/shell-api";

import type { Flavor } from "../../shared/flavor";
import { socketPort } from "./socket-port";

/** An attempt that failed, with the code `ConnectionState`'s `failed.reason` takes. */
export class HostFailure extends Error {
  override name = "HostFailure";

  constructor(
    readonly reason: ConnectionFailure,
    options?: ErrorOptions,
  ) {
    super(`host connection failed: ${reason}`, options);
  }
}

export interface Connected extends Handshaken {
  port: HostPort;
  /** Closes when the host or the extension ends the connection. */
  socket: Socket;
  conn: HostConnection;
}

export interface ConnectOptions {
  role: ClientRole;
  features: Feature[];
  flavor: Flavor;
  /** The app's version, in `initialize`'s `client`. */
  version: string;
  /** How long the handshake may take. */
  timeoutMs: number;
}

/** The handshake on `port`; `keyed` is the extension's connection in keyed builds. */
export function handshakeHost(
  port: HostPort,
  keyed: KeyedHostConnection | undefined,
  options: ConnectOptions,
  signal: AbortSignal,
): Promise<Handshaken> {
  return handshake(port, {
    client: { kind: "desktop", name: "Cupper", version: options.version },
    role: options.role,
    features: options.features,
    auth: (hello) => keyed?.auth(hello.nonce) ?? { type: "codesign" },
    verifyHostProof: (proof) => keyed?.verifyHostProof(proof) ?? false,
    refuseWeakAuth: options.flavor === "release",
    signal,
  });
}

/**
 * Runs the handshake on `conn`, a connection the extension's `connectHost`
 * made; its failures become a {@link HostFailure}.
 */
export async function openConnection(
  conn: HostConnection,
  keyed: boolean,
  options: ConnectOptions,
): Promise<Connected> {
  const socket = new Socket({ fd: conn.takeFd(), readable: true, writable: true });
  const port = socketPort(socket);
  const closed = new AbortController();
  socket.once("close", () => closed.abort());
  const signal = AbortSignal.any([closed.signal, AbortSignal.timeout(options.timeoutMs)]);
  const keyedConn = keyed ? (conn as KeyedHostConnection) : undefined;
  try {
    return { ...(await handshakeHost(port, keyedConn, options, signal)), port, socket, conn };
  } catch (error) {
    // Without a `host_proof` the extension keeps the connection (T073).
    conn.close();
    socket.destroy();
    const reason = error instanceof HandshakeError ? error.reason : "protocol_error";
    throw new HostFailure(reason, { cause: error });
  }
}
