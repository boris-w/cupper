// What `cupper_native.node` exports (`src/lib.rs`). The main process loads it
// at run time, from outside the asar (design 5.1.3), so this file only
// declares it. Which interface the loaded module has depends on its build
// (design 5.7.9): `ReleaseNative` without features, `DevAuthNative` with
// `dev-auth` (development, self-use and e2e builds). `buildInfo()` tells.
//
// Errors thrown by the module carry a semantic `code`: `invalid_argument`,
// `spawn_failed`, `connect_failed`, `fd_taken`, `already_authenticated`,
// `protocol_error`, `unexpected`, or one of `HostRefusal`.

export interface BuildInfo {
  /** Compile-time features; none in a release build. */
  features: string[];
  /** `signature`: release; `key_proof`: `dev-auth` (design 5.8.1). */
  hostCheck: "signature" | "key_proof";
}

/** How to start the host (design 5.7.2, 5.8.5). */
export interface HostCommand {
  /** An absolute path; there is no `PATH` search. */
  program: string;
  args: string[];
  /** The host's whole environment; `CUPPER_*` names are refused. */
  env: Record<string, string>;
  /** A pipe instead of `/dev/null`; the caller owns the fd it gets. */
  pipeStdout?: boolean;
  pipeStderr?: boolean;
}

/** A started host, in a session of its own. */
export interface HostProcess {
  pid: number;
  stdout?: number;
  stderr?: number;
  /** Resolves once the host has exited; the extension reaps it. */
  exit: Promise<HostExit>;
}

export interface HostExit {
  code?: number;
  signal?: number;
}

/** Why `connectHost` refused the host: the `code` of its error (design 5.8.1). */
export type HostRefusal =
  | "peer_exited"
  | "peer_execed"
  | "code_changed"
  | "unverifiable"
  | "not_the_host"
  | "other_user"
  | "no_requirement";

/** Why a connection ended. */
export type CloseReason =
  "host_closed" | "fds_passed" | "token_changed" | "host_unverified" | "local";

/**
 * A connection to a host that passed the check on connect. The main process
 * wraps its end in `net.Socket`; every read of the host goes through the
 * extension first (fds closed, pinned token compared). When that socket
 * reads end of file, `closeReason` says why.
 */
export interface HostConnection {
  /** The main process's end, once: `new net.Socket({ fd })`. */
  takeFd(): number;
  /** The host's pid, from the pinned token. */
  readonly hostPid: number;
  /** Null while the connection is open. */
  readonly closeReason: CloseReason | null;
  close(): void;
}

/** A connection in a `dev-auth` build: the host still has to prove the key. */
export interface KeyedHostConnection extends HostConnection {
  /** `initialize`'s `auth` for the `nonce` of `host/hello`; once. */
  auth(nonce: string): DevKeyAuth;
  /**
   * Whether `host_proof` from `initialize`'s result proves the key. When it
   * does not, or is missing, the extension closes the connection.
   */
  verifyHostProof(hostProof?: string | null): boolean;
}

export interface DevKeyAuth {
  type: "dev_key";
  c_nonce: string;
  proof: string;
}

declare const hostKey: unique symbol;

/** A handle to the one-time key; the key itself never leaves the extension. */
export interface HostKey {
  readonly [hostKey]: true;
}

interface NativeBase {
  buildInfo(): BuildInfo;
  /** Where the host of `dataDir` listens (design 5.2.13). */
  socketPath(dataDir: string): string;
}

/** A release build: no startup payload; the host must satisfy the requirement. */
export interface ReleaseNative extends NativeBase {
  spawnHost(command: HostCommand): HostProcess;
  connectHost(socketPath: string): HostConnection;
}

/** A `dev-auth` build: the startup payload carries `key` as `dev_key`. */
export interface DevAuthNative extends NativeBase {
  HostKey: new () => HostKey;
  /** `payload`: further entries of the startup payload (e2e: `harness_key`, `test_config`). */
  spawnHost(command: HostCommand, key: HostKey, payload?: Record<string, unknown>): HostProcess;
  connectHost(socketPath: string, key: HostKey): KeyedHostConnection;
}
