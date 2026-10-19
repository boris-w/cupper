// The ShellAPI of design 5.7.3: everything the UI asks of its shell. `ui-core`
// depends on this interface only, never on Electron. The preload exposes the
// Electron implementation; `./browser-mock` implements it for screenshot tests.
// The interface is frozen in M1a and only gains members afterwards (design 5.12.5).

import type {
  ElevationId,
  InitializeResult,
  ProjectId,
  RequestId,
  SessionCreateParams,
  SessionId,
  SettingsPatch,
} from "@cupper/protocol";

/** Cancels a subscription. Calling it again does nothing. */
export type Unsubscribe = () => void;

export type Theme = "light" | "dark";

export interface ShellAPI {
  /** Called once at startup: the static facts in one round trip, without `sendSync`. */
  init(): Promise<ShellInfo>;
  /** The data connection of this document (design 5.7.1). Sockets and credentials stay in the main process. */
  host: {
    /** Opens the connection; the main process makes the handshake (role `window`) before handing over the port. */
    open(): Promise<{ port: HostPort; handshake: Handshake }>;
    /**
     * The connection's state. After a reconnect, `ready` comes with the new
     * handshake, and the UI subscribes again from its cursors.
     */
    onConnectionState(cb: (state: ConnectionState, handshake?: Handshake) => void): Unsubscribe;
  };
  /** An action behind a native confirmation dialog; it needs real input (design 5.7.8, rule 8). */
  requestPrivileged(action: PrivilegedAction): Promise<PrivilegedResult>;
  /** Menu accelerators and other commands, also while the focus is in the review document. */
  onCommand(cb: (cmd: ShellCommand) => void): Unsubscribe;
  /** Deep links and notification clicks. They only navigate, and never focus the request panel. */
  onNavigate(cb: (target: NavigateTarget) => void): Unsubscribe;
  /** Opens a link in the default browser. Takes http and https only; the main process checks again. */
  openExternal(url: string): void;
  /** Asks the user for a directory, e.g. the worktree root; `null` when cancelled. */
  pickDirectory(): Promise<string | null>;
  system: {
    /** The system appearance changed; the setting may pin the UI to one theme instead. */
    onThemeChange(cb: (theme: Theme) => void): Unsubscribe;
  };
  window: {
    /**
     * The window gained or lost focus. `at` is when, in milliseconds since the
     * Unix epoch: key presses within 1 s of gaining focus are not real input
     * (design 5.7.8, rule 7), and the request panel waits that out.
     */
    onFocusChange(cb: (focused: boolean, at: number) => void): Unsubscribe;
  };
  /** UI preferences, kept by the main process. `ShellInfo.prefs` is the initial snapshot. */
  prefs: {
    set(key: string, value: unknown): Promise<void>;
    /** A preference changed, from this window or another one. */
    onChange(cb: (key: string, value: unknown) => void): Unsubscribe;
  };
  updates: {
    onState(cb: (state: UpdateState) => void): Unsubscribe;
    /** "Restart to update": installs a downloaded update (design 5.10.5). */
    installNow(): void;
  };
}

export interface ShellInfo {
  shell: "electron" | "browser";
  flavor: "release" | "e2e" | "dev";
  appVersion: string;
  /** BCP 47 tag: the system's preferred language unless the settings override it. */
  locale: string;
  theme: Theme;
  /** The preferences at startup; later changes arrive through `prefs.onChange`. */
  prefs: Record<string, unknown>;
  updateState: UpdateState;
  /** Where the review document loads from. It differs under the browser mock, so `ui-core` never hard-codes it. */
  reviewDocumentUrl: string;
}

/** Commands the menu sends today (design 5.7.7). */
export type KnownCommandId =
  | "new_session"
  | "settings"
  | "toggle_sidebar"
  | "toggle_side_panel"
  | "open_review_tab"
  | "toggle_file_tree"
  | "interrupt"
  | "next_needs_action"
  | "prev_sidebar_item"
  | "next_sidebar_item"
  | "prev_recent"
  | "next_recent"
  | "session_n"
  | "archive"
  | "rename"
  | "model_picker"
  | "open_in_terminal";

/**
 * An open enum: a new command does not change the interface, and the UI
 * ignores commands it does not know (see `commandHandler`). `string & {}`
 * keeps the known IDs in autocompletion.
 */
export type ShellCommandId = KnownCommandId | (string & {});

export interface ShellCommand {
  id: ShellCommandId;
  /** `session_n`: which sidebar session, 1–9. */
  arg?: number;
}

export type KnownNavigateTarget =
  | { kind: "session"; session_id: string; request_id?: string }
  | { kind: "settings"; section?: string };

/**
 * An open enum like `ShellCommandId`. The open member stops `kind` from
 * narrowing the union, so narrow with `isKnownTarget` first.
 */
export type NavigateTarget = KnownNavigateTarget | { kind: string & {} };

/**
 * Where the updater is (design 5.10.5). Updates download in the background;
 * `ready` is "restart to update", and `installing` is the window between
 * `installNow` and the app quitting, when the host is down on purpose.
 */
export type UpdateState =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "downloading"; version: string }
  | { status: "ready"; version: string }
  | { status: "installing"; version: string }
  | { status: "error" };

/**
 * The window's end of its data connection. The main process forwards inbound
 * bytes without parsing them, and parses, checks and re-serializes every
 * outbound message (design 5.7.1).
 */
export interface HostPort {
  /**
   * One JSON-RPC message. A request the main process refuses, such as
   * `initialize`, is answered through `onData` with an error response
   * (`forbidden`); a refused message without an `id`, a batch array or
   * anything that is not JSON is dropped without a reply.
   */
  send(json: string): void;
  /** One `0x02` chunk of an upload; `hash` is the sha-256 of the whole blob in hex. */
  sendBlob(hash: string, offset: number, last: boolean, bytes: ArrayBuffer): void;
  /**
   * Inbound bytes, each chunk its own `ArrayBuffer`, split anywhere: the
   * host-client cuts the frames. On a reconnect, every chunk of the old
   * connection arrives before `reconnecting`, and the new one's only after `ready`.
   */
  onData(cb: (chunk: ArrayBuffer) => void): Unsubscribe;
  /** Bytes handled, right after they were applied (flow control, design 5.7.1). */
  ack(bytes: number): void;
  /** Closes the connection for good: no reconnect follows. */
  close(): void;
}

/** The result of `initialize`, without the host's proof, which the main process checked. */
export type Handshake = Omit<InitializeResult, "host_proof">;

/** Failures a client can tell from the handshake (see `@cupper/host-client`). */
export type KnownConnectionFailure =
  | "version_unsupported"
  | "unauthenticated"
  | "forbidden"
  | "weak_auth"
  | "host_unverified"
  | "protocol_error";

/** A semantic code; the main process adds its own, such as for a host that does not start. */
export type ConnectionFailure = KnownConnectionFailure | (string & {});

/** Where the connection is: `reconnecting` covers every attempt after the first `ready`. */
export type ConnectionState =
  | { status: "connecting" | "handshaking" | "ready" | "reconnecting" }
  | { status: "failed"; reason: ConnectionFailure };

export type PrivilegedAction =
  | { kind: "interaction"; session_id: SessionId; request_id: RequestId; option_id: string }
  | { kind: "set_config"; session_id: SessionId; option: string; value: string }
  | { kind: "confirm_config"; session_id: SessionId }
  | { kind: "create_session"; params: SessionCreateParams }
  /** Picks a directory, adds the project and confirms trust in one go. */
  | { kind: "add_project" }
  | { kind: "trust_project"; project_id: ProjectId }
  | { kind: "elevation"; elevation_id: ElevationId }
  | { kind: "settings"; patch: SettingsPatch };

export type PrivilegedResult =
  /** `create_session` comes with `{session_id}`, `add_project` with `{project_id}`. */
  | { status: "confirmed"; result?: unknown }
  | { status: "cancelled" }
  /** Another confirmation dialog is open. */
  | { status: "busy" }
  /** Cancelled too often lately (2 s, 10 s, 60 s). */
  | { status: "cooldown"; retry_after_ms: number }
  | { status: "user_activation_required" }
  | { status: "failed"; error: unknown };
