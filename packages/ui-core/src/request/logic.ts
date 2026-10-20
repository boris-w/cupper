// The request panel's rules (design 5.7.4, 5.7.8 RP-3 to RP-7) as pure
// functions: what is selectable, what a key does, what "skip" answers.

import type { InteractionOption, InteractionOutcome, InteractionRequest } from "@cupper/protocol";

/** The kinds the panel answers; `auth_url` (not produced in M1) and unknown kinds only close. */
export type Handled = Extract<
  InteractionRequest,
  { type: "permission" | "question" | "plan_confirm" }
>;
const HANDLED = new Set(["permission", "question", "plan_confirm"]);
export const handled = (request: InteractionRequest): request is Handled =>
  typeof request.type === "string" && HANDLED.has(request.type);

/** Only through the native dialog: `requires: privileged`, and every option of unknown effect. */
export const isPrivileged = (option: InteractionOption) =>
  option.requires === "privileged" || option.effect.scope === "unknown";

const isReject = (option: InteractionOption) =>
  option.effect.scope === "reject" || option.effect.scope === "plan_revise";

/** Allowing options stay off while truncated content was not seen in full (security #6). */
export const isEnabled = (option: InteractionOption, truncated: boolean) =>
  !truncated || isReject(option);

/** RP-4: the first option that is neither privileged nor of unknown effect; -1 when none is. */
export const initialSelection = (options: InteractionOption[], truncated = false) =>
  options.findIndex((option) => !isPrivileged(option) && isEnabled(option, truncated));

/** RP-6: ↑ ↓ move to the next option, skipping those of unknown effect and disabled ones. */
export function move(options: InteractionOption[], from: number, step: 1 | -1, truncated = false) {
  for (let at = from + step; at >= 0 && at < options.length; at += step) {
    const option = options[at];
    if (option && option.effect.scope !== "unknown" && isEnabled(option, truncated)) {
      return at;
    }
  }
  return from;
}

/**
 * "跳过" and "关闭" (design 5.7.4): the non-persistent rejecting option of a
 * permission or a plan, else `dismissed`. A question's skip is per field and
 * left to the question; its close is `dismissed`.
 */
export function skipOutcome(request: InteractionRequest): InteractionOutcome {
  const options = "options" in request && Array.isArray(request.options) ? request.options : [];
  const reject = (options as InteractionOption[]).find((o) => isReject(o) && !o.effect.persistent);
  return reject ? { type: "selected", option_id: reject.option_id } : { type: "dismissed" };
}

export type PanelKey =
  | { type: "digit"; index: number }
  | { type: "up" | "down" | "left" | "right" | "submit" | "cmd_submit" | "close" | "tab" }
  | { type: "typing" }
  | { type: "other" };

/**
 * What a key means on the panel. Digits, arrows, Enter, ⌘Enter, Esc and Tab
 * work the panel; any other key that types is a typing key (RP-3), which
 * revokes keyboard activation; modifiers alone and composition are neither.
 */
export function panelKey(event: {
  key: string;
  isComposing?: boolean;
  metaKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
}): PanelKey {
  if (event.isComposing) {
    return { type: "typing" };
  }
  const plain = !event.metaKey && !event.ctrlKey && !event.altKey;
  if (/^[1-9]$/.test(event.key) && plain) {
    return { type: "digit", index: Number(event.key) - 1 };
  }
  const named: Record<string, PanelKey["type"]> = {
    ArrowUp: "up",
    ArrowDown: "down",
    ArrowLeft: "left",
    ArrowRight: "right",
    Escape: "close",
    Tab: "tab",
  };
  if (event.key === "Enter") {
    return { type: event.metaKey || event.ctrlKey ? "cmd_submit" : "submit" };
  }
  const type = named[event.key];
  if (type) {
    return { type } as PanelKey;
  }
  return event.key.length === 1 || event.key === "Backspace" || event.key === "Delete"
    ? { type: "typing" }
    : { type: "other" };
}
