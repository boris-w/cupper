// The composer's keys (design 5.7.4, 5.7.7) as pure functions.

/** The "send shortcut" preference: Enter, or ⌘Enter with Enter for a newline. */
export type SendKey = "enter" | "mod_enter";
export const SEND_KEY_PREF = "composer.sendKey";

export interface KeyLike {
  key: string;
  keyCode?: number;
  isComposing?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
  metaKey?: boolean;
  ctrlKey?: boolean;
}

/**
 * Whether a key press in the text area sends. Never while an input method
 * composes, Enter picking a candidate included (CP-1: `isComposing`, or
 * keyCode 229). ⇧Enter and ⌥Enter keep their newline. M1a has no steer, so
 * ⌘Enter and ⌘⇧Enter, the queue keys, send as well: every send queues.
 */
export function sends(event: KeyLike, sendKey: SendKey): boolean {
  if (event.key !== "Enter" || event.isComposing || event.keyCode === 229) {
    return false;
  }
  const mod = Boolean(event.metaKey || event.ctrlKey);
  if (sendKey === "mod_enter" || mod) {
    return mod && !event.altKey;
  }
  return !event.shiftKey && !event.altKey;
}

export type EscapeAction = "close_overlay" | "arm" | "interrupt" | "focus" | "none";

/**
 * Esc, in Codex's order: an overlay in the composer closes first; while a
 * turn runs, the first Esc arms "press Esc again to stop" and the second
 * interrupts; from outside the composer, Esc brings the focus back.
 */
export function escapeAction(state: {
  focused: boolean;
  overlayOpen: boolean;
  running: boolean;
  armed: boolean;
}): EscapeAction {
  if (!state.focused) {
    return "focus";
  }
  if (state.overlayOpen) {
    return "close_overlay";
  }
  if (!state.running) {
    return "none";
  }
  return state.armed ? "interrupt" : "arm";
}
