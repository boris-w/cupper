// The composer's keys (design 5.7.4, 5.7.7) as pure functions.

/** The "send shortcut" preference: Enter, or ⌘Enter with Enter for a newline. */
export type SendKey = "enter" | "mod_enter";
export const SEND_KEY_PREF = "composer.sendKey";
/** "Follow-ups queue by default" (design 5.7.4): on, the send key queues and the queue key steers. */
export const FOLLOW_UP_QUEUE_PREF = "composer.followUpQueue";

export interface KeyLike {
  key: string;
  keyCode?: number;
  isComposing?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
  metaKey?: boolean;
  ctrlKey?: boolean;
}

/** Whether an input method composes, the key picking a candidate included (CP-1). */
export const composing = (event: KeyLike) => Boolean(event.isComposing) || event.keyCode === 229;

/** What the send key and the queue key ask for; while a turn runs, "send" is to steer. */
export type SendAction = "send" | "queue";

/**
 * What a key press in the text area asks for (design 5.7.7): the send key,
 * Enter or ⌘Enter as the preference says, sends; the queue key, ⌘Enter or
 * ⌘⇧Enter when ⌘Enter sends, queues. Nothing while an input method composes
 * (`composing`); ⇧Enter and ⌥Enter keep their newline, and Enter too when
 * ⌘Enter sends.
 */
export function sendAction(event: KeyLike, sendKey: SendKey): SendAction | undefined {
  if (event.key !== "Enter" || composing(event) || event.altKey) {
    return undefined;
  }
  const mod = Boolean(event.metaKey || event.ctrlKey);
  if (sendKey === "mod_enter") {
    return mod ? (event.shiftKey ? "queue" : "send") : undefined;
  }
  return mod ? "queue" : event.shiftKey ? undefined : "send";
}

/**
 * Whether a message asks to steer the running turn rather than queue behind
 * it: the send key does, or the queue key with follow-ups queued by default.
 * Idle, nothing steers; a message starts a turn.
 */
export const steers = (action: SendAction, running: boolean, followUpQueue: boolean) =>
  running && (action === "send") !== followUpQueue;

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
