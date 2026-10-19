// The ShellAPI of design 5.7.3: everything the UI asks of its shell. `ui-core`
// depends on this interface only, never on Electron. The preload exposes the
// Electron implementation; `./browser-mock` implements it for screenshot tests.
//
// The members that carry protocol types (`host.*`, `HostPort`,
// `requestPrivileged`) are added by T071. The interface is frozen in M1a and
// only gains members afterwards (design 5.12.5).

/** Cancels a subscription. Calling it again does nothing. */
export type Unsubscribe = () => void;

export type Theme = "light" | "dark";

export interface ShellAPI {
  /** Called once at startup: the static facts in one round trip, without `sendSync`. */
  init(): Promise<ShellInfo>;
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
