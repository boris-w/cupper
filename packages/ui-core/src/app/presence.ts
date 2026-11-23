// The window's presence (design 5.2.9, 5.3): whether it is in front and the
// session it shows, as `client/presence {foreground, session_id?}` on the
// window's own connection, which the host takes from a `desktop_ui` only. A
// foreground window showing a session keeps that session's notifications
// away. Reported when the focus or the page changes and once more on each
// new connection, which the host knows nothing of; never the same twice on
// one connection. `useFocused` is the focus signal, for anything else that
// follows the window coming to the front. The same judgment marks the session
// in front read (U4, T170).

import type { ClientPresenceParams } from "@cupper/protocol";
import { useEffect, useRef, useState } from "react";
import { useStore } from "zustand";

import { useUi } from "../state/provider";
import { useHost } from "./providers";

/**
 * Whether the window has the focus, in any of its frames, following it. A
 * `blur` comes too when the focus goes into one of the window's frames, such
 * as the review document: the window is still in front, and
 * `document.hasFocus()` stays true while a frame of it has the focus. So a
 * blur is checked with it, once the move is done.
 */
export function useFocused(): boolean {
  const [focused, setFocused] = useState(() => document.hasFocus());
  useEffect(() => {
    let later: ReturnType<typeof setTimeout> | undefined;
    const check = () => setFocused(document.hasFocus());
    const blurred = () => {
      clearTimeout(later);
      later = setTimeout(check, 0);
    };
    window.addEventListener("focus", check);
    window.addEventListener("blur", blurred);
    return () => {
      clearTimeout(later);
      window.removeEventListener("focus", check);
      window.removeEventListener("blur", blurred);
    };
  }, []);
  return focused;
}

/** The least time between two read marks of the session in front while its events come. */
export const READ_MARK_MS = 1000;

/**
 * Marks session `sessionId` read up to the seq its view reached, while the
 * window shows it in front (design 5.2.9, U4): as it comes in front, then as
 * its stream moves, at most once each `READ_MARK_MS`, and once more as it
 * leaves for what came in the meantime.
 */
function useReadMark(foreground: boolean, sessionId: string | undefined): void {
  const { client } = useHost();
  useEffect(() => {
    if (!foreground || sessionId === undefined) return;
    const stream = `session/${sessionId}`;
    let told = 0;
    let pending = false;
    let cooling: ReturnType<typeof setTimeout> | undefined;
    const tell = () => {
      const seq = client.seq(stream);
      if (seq === undefined || seq <= told || client.state.status !== "ready") return false;
      told = seq;
      client.call("session/mark_read", { session_id: sessionId, up_to_seq: seq }, () => undefined);
      return true;
    };
    const mark = () => {
      if (!tell()) return;
      cooling = setTimeout(() => {
        cooling = undefined;
        if (pending) {
          pending = false;
          mark();
        }
      }, READ_MARK_MS);
    };
    mark();
    // The streams see each notification before this listener: the seq has moved.
    const off = client.onNotification((_, params) => {
      if ((params as { stream?: unknown }).stream !== stream) return;
      if (cooling === undefined) mark();
      else pending = true;
    });
    return () => {
      off();
      clearTimeout(cooling);
      if (pending) tell();
    };
  }, [client, foreground, sessionId]);
}

/** Reports the window's presence to the host; mounted once, in the app. */
export function usePresence(): void {
  const { client } = useHost();
  const foreground = useFocused();
  const sessionId = useStore(useUi(), ({ route }) =>
    route.page === "session" ? route.sessionId : undefined,
  );
  // What this connection was told last; nothing yet on a new one.
  const told = useRef<string | undefined>(undefined);
  useEffect(() => {
    const params: ClientPresenceParams = {
      foreground,
      ...(sessionId === undefined ? {} : { session_id: sessionId }),
    };
    const report = () => {
      const said = JSON.stringify(params);
      if (client.state.status !== "ready" || said === told.current) return;
      told.current = said;
      client.notify("client/presence", params);
    };
    report();
    // Each "ready" is a new connection, and anything else none: either way the host knows nothing yet.
    return client.onState(() => {
      told.current = undefined;
      report();
    });
  }, [client, foreground, sessionId]);
  useReadMark(foreground, sessionId);
}

/** Renders nothing: the app's presence reporter. */
export function Presence(): null {
  usePresence();
  return null;
}
