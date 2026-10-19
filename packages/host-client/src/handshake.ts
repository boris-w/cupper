// The handshake (design 5.3.2, 5.8.1): the host speaks first with
// `host/hello`; the client answers with `initialize`, once. The main process
// runs it for both of its connections (T075, T076). Proofs are not made here:
// the caller supplies them, from the native extension (T073).

import {
  zHostHelloParams,
  zInitializeResult,
  zRpcError,
  type Auth,
  type ClientIdentity,
  type ClientRole,
  type Feature,
  type HostHelloParams,
  type RpcError,
  type VersionRange,
} from "@cupper/protocol";
import type { Handshake, HostPort, KnownConnectionFailure } from "@cupper/shell-api";

import { FRAME_JSON, FrameDecoder } from "./frame";

/** The major protocol versions this client speaks. */
export const PROTOCOL: VersionRange = { min: 1, max: 1 };

export interface HandshakeOptions {
  client: ClientIdentity;
  /** `control` or `window`; desktop connections only. */
  role?: ClientRole;
  features: Feature[];
  coalesce_ms?: number;
  /** The credentials for this hello, e.g. a `dev_key` proof over its nonce. */
  auth(hello: HostHelloParams): Auth | Promise<Auth>;
  /** Checks `host_proof`, which key-based authentication (`dev_key`, `harness`) requires. */
  verifyHostProof(proof: string, hello: HostHelloParams, auth: Auth): boolean | Promise<boolean>;
  /** Release builds refuse a host in the degraded mode (`weak_auth`, design 5.8.1). */
  refuseWeakAuth: boolean;
  /** Gives up, e.g. when the socket closed or on a timeout (`AbortSignal.timeout`). */
  signal?: AbortSignal;
}

/** `closed`: the connection went away; trying again may work. The others are failures. */
export type HandshakeFailure = KnownConnectionFailure | "closed";

export class HandshakeError extends Error {
  override name = "HandshakeError";

  constructor(
    readonly reason: HandshakeFailure,
    /** The host's error, when it refused `initialize`. */
    readonly error?: RpcError,
  ) {
    super(error ? `handshake failed: ${reason} (${error.message})` : `handshake failed: ${reason}`);
  }
}

export interface Handshaken {
  handshake: Handshake;
  /** Bytes that came after the response to `initialize`; the next reader starts with them. */
  rest: Uint8Array;
}

/** Codes of a refused `initialize` that end the attempt as they are. */
const REFUSALS: readonly string[] = ["version_unsupported", "unauthenticated", "forbidden"];

export async function handshake(port: HostPort, options: HandshakeOptions): Promise<Handshaken> {
  const { signal } = options;
  const decoder = new FrameDecoder();
  let wake: (() => void) | undefined;
  const off = port.onData((chunk) => {
    decoder.push(new Uint8Array(chunk));
    wake?.();
  });
  const onAbort = () => wake?.();
  signal?.addEventListener("abort", onAbort);

  /** The next JSON message, frame by frame, so that what follows stays in `decoder`. */
  async function next(): Promise<Record<string, unknown>> {
    for (;;) {
      if (signal?.aborted) {
        throw new HandshakeError("closed");
      }
      let frame;
      try {
        frame = decoder.next();
      } catch {
        throw new HandshakeError("protocol_error");
      }
      if (frame?.type === FRAME_JSON) {
        let message: unknown;
        try {
          message = JSON.parse(new TextDecoder().decode(frame.payload));
        } catch {
          throw new HandshakeError("protocol_error");
        }
        if (typeof message !== "object" || message === null || Array.isArray(message)) {
          throw new HandshakeError("protocol_error");
        }
        return message as Record<string, unknown>;
      }
      if (frame === undefined) {
        await new Promise<void>((resolve) => (wake = resolve));
      }
    }
  }

  try {
    const first = await next();
    const hello = zHostHelloParams.safeParse(first.params);
    if (first.method !== "host/hello" || "id" in first || !hello.success) {
      throw new HandshakeError("protocol_error");
    }
    const version = Math.min(PROTOCOL.max, hello.data.protocol.max);
    if (version < Math.max(PROTOCOL.min, hello.data.protocol.min)) {
      throw new HandshakeError("version_unsupported");
    }
    const auth = await options.auth(hello.data);
    const params = {
      protocol: PROTOCOL,
      nonce: hello.data.nonce,
      client: options.client,
      ...(options.role === undefined ? {} : { role: options.role }),
      auth,
      features: options.features,
      ...(options.coalesce_ms === undefined ? {} : { coalesce_ms: options.coalesce_ms }),
    };
    port.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params }));

    const response = await next();
    if (response.id !== 1 || "method" in response) {
      throw new HandshakeError("protocol_error");
    }
    if ("error" in response) {
      const error = zRpcError.safeParse(response.error);
      const code = error.success ? error.data.data?.code : undefined;
      const reason = code === "shutting_down" ? "closed" : "protocol_error";
      throw new HandshakeError(
        code !== undefined && REFUSALS.includes(code) ? (code as HandshakeFailure) : reason,
        error.data,
      );
    }
    const result = zInitializeResult.safeParse(response.result);
    if (!result.success || result.data.protocol !== version) {
      throw new HandshakeError("protocol_error");
    }
    const { host_proof: proof, ...rest } = result.data;
    if (rest.host.weak_auth && options.refuseWeakAuth) {
      throw new HandshakeError("weak_auth");
    }
    if (auth.type === "dev_key" || auth.type === "harness") {
      if (typeof proof !== "string" || !(await options.verifyHostProof(proof, hello.data, auth))) {
        throw new HandshakeError("host_unverified");
      }
    }
    return { handshake: rest, rest: decoder.rest() };
  } finally {
    off();
    signal?.removeEventListener("abort", onAbort);
  }
}
