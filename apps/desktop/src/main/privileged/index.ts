// privileged (design 5.7.2; 5.7.8, rule 8 and the native dialogs): the
// ShellAPI's `requestPrivileged`. It needs real input that is not Escape.
// One dialog at a time: the others are `busy`. After the user cancels, the
// same kind of action waits 2 s, then 10 s, then 60 s (`cooldown`), until a
// confirmation, or 10 minutes without a cancel, starts the count again.

import { ConnectionLost, RpcFailure } from "@cupper/host-client";
import type { InputEvidence } from "@cupper/protocol";
import type { PrivilegedAction, PrivilegedResult } from "@cupper/shell-api";

import type { Translate } from "../host/dialogs";
import { run, type PrivilegedUi } from "./actions";
import type { PrivilegedHost } from "./host";

export type { PrivilegedUi } from "./actions";
export { controlHost, type PrivilegedHost } from "./host";

export const COOLDOWNS_MS = [2_000, 10_000, 60_000] as const;
export const COOLDOWN_RESET_MS = 10 * 60_000;

export interface PrivilegedDeps {
  /** Milliseconds; injectable for the tests. */
  now(): number;
  host: PrivilegedHost;
  /** `privileged` text by its key. */
  t: Translate;
}

export class Privileged {
  readonly #deps: PrivilegedDeps;
  #open = false;
  /** By action kind: the cancels in a row, and when the latest was. */
  readonly #cancels = new Map<string, { count: number; at: number }>();

  constructor(deps: PrivilegedDeps) {
    this.#deps = deps;
  }

  /** `action` from a window, with the input it took for it. */
  async request(
    input: InputEvidence | undefined,
    action: unknown,
    ui: PrivilegedUi,
  ): Promise<PrivilegedResult> {
    if (input === undefined || input.key === "escape") {
      return { status: "user_activation_required" };
    }
    const checked = parseAction(action);
    if (checked === undefined) {
      return { status: "failed", error: { code: "invalid_params" } };
    }
    if (this.#open) {
      return { status: "busy" };
    }
    const wait = this.#cooldown(checked.kind);
    if (wait > 0) {
      return { status: "cooldown", retry_after_ms: wait };
    }
    this.#open = true;
    try {
      const { host, t } = this.#deps;
      const result = await run(checked, { host, ui, t });
      this.#count(checked.kind, result.status === "cancelled");
      return result;
    } catch (error) {
      return { status: "failed", error: failure(error) };
    } finally {
      this.#open = false;
    }
  }

  #cooldown(kind: string): number {
    const cancels = this.#cancels.get(kind);
    const now = this.#deps.now();
    if (cancels === undefined || now - cancels.at >= COOLDOWN_RESET_MS) {
      return 0;
    }
    const wait = COOLDOWNS_MS[Math.min(cancels.count, COOLDOWNS_MS.length) - 1] ?? 0;
    return Math.max(0, cancels.at + wait - now);
  }

  #count(kind: string, cancelled: boolean): void {
    if (!cancelled) {
      this.#cancels.delete(kind);
      return;
    }
    const now = this.#deps.now();
    const before = this.#cancels.get(kind);
    const running = before !== undefined && now - before.at < COOLDOWN_RESET_MS;
    this.#cancels.set(kind, { count: running ? before.count + 1 : 1, at: now });
  }
}

/** What the host or the connection said, as codes. */
function failure(error: unknown): { code: string; reason?: string } {
  if (error instanceof RpcFailure) {
    const { code, reason } = error.error.data ?? {};
    return typeof reason === "string" ? { code: String(code), reason } : { code: String(code) };
  }
  return { code: error instanceof ConnectionLost ? "connection_lost" : "unexpected" };
}

const STRINGS: Record<string, string[]> = {
  interaction: ["session_id", "request_id", "option_id"],
  set_config: ["session_id", "option", "value"],
  confirm_config: ["session_id"],
  create_session: [],
  add_project: [],
  trust_project: ["project_id"],
  elevation: ["elevation_id"],
  settings: [],
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** The renderer's action, when it has the fields of its kind. */
export function parseAction(value: unknown): PrivilegedAction | undefined {
  if (!isRecord(value) || typeof value.kind !== "string" || !Object.hasOwn(STRINGS, value.kind)) {
    return undefined;
  }
  const strings = STRINGS[value.kind] ?? [];
  const records = { create_session: "params", settings: "patch" }[value.kind as string];
  const valid =
    strings.every((key) => typeof value[key] === "string") &&
    (records === undefined || isRecord(value[records]));
  return valid ? (value as PrivilegedAction) : undefined;
}
