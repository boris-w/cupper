// deeplink (design 5.7.2, 5.7.8): the one route of this build kind's URL
// scheme, `<scheme>://session/<session_id>[?request=<request_id>]`, which the
// host hands the app after a click on a notification when the UI is not
// running (design 5.2.9), and the host's `ui/navigate` when it is. Both only
// navigate the main window to the session; the request panel keeps no focus.
// Anything else, another build kind's scheme included, is ignored.

import type { NavigateTarget } from "@cupper/shell-api";

import type { Flavor } from "../shared/flavor";

/** Each build kind's URL scheme (design 5.7.9). */
export const SCHEMES: Record<Flavor, string> = {
  release: "cupper",
  local: "cupper-local",
  e2e: "cupper-e2e",
  dev: "cupper-dev",
};

/** A protocol ID: its prefix, then 1 to 64 of `[A-Za-z0-9_-]` (design 5.4.4). */
const isId = (prefix: string, text: unknown): text is string =>
  typeof text === "string" && new RegExp(`^${prefix}[A-Za-z0-9_-]{1,64}$`).test(text);

/** The session target of well-formed IDs; `null` otherwise. */
export function sessionTarget(session: unknown, request?: unknown): NavigateTarget | null {
  if (!isId("s_", session) || (request !== undefined && !isId("r_", request))) {
    return null;
  }
  return request === undefined
    ? { kind: "session", session_id: session }
    : { kind: "session", session_id: session, request_id: request };
}

/** Where `url`, a link of `scheme`, goes; `null` for anything but the one route. */
export function deepLink(url: string, scheme: string): NavigateTarget | null {
  let link: URL;
  try {
    link = new URL(url);
  } catch {
    return null;
  }
  const extra = link.username || link.password || link.port || link.hash;
  const query = [...link.searchParams.keys()];
  if (link.protocol !== `${scheme}:` || link.host !== "session" || extra) {
    return null;
  }
  if (query.some((key) => key !== "request") || query.length > 1) {
    return null;
  }
  return sessionTarget(link.pathname.slice(1), link.searchParams.get("request") ?? undefined);
}

/**
 * The main window's latest target, sent to each document the window loads
 * until one of them has its data connection. One that comes before the app
 * has a window, as a link that started it, waits for the first; a reload, as
 * after restarting the host (T215), starts the route anew, and would drop a
 * target the document before it took. A newer target replaces an older one.
 */
export class WindowNavigation {
  readonly #send: (target: NavigateTarget) => void;
  #target: NavigateTarget | undefined;
  /** Of the current document: loaded, sent the target, has its connection. */
  #loaded = false;
  #sent = false;
  #connected = false;

  constructor(send: (target: NavigateTarget) => void) {
    this.#send = send;
  }

  go(target: NavigateTarget): void {
    this.#target = target;
    this.#sent = false;
    this.#deliver();
  }

  /** The window's document finished loading, a reload's included. */
  loaded(): void {
    this.#loaded = true;
    this.#deliver();
  }

  /** The current document opened its data connection. */
  connected(): void {
    this.#connected = true;
    this.#deliver();
  }

  /** The window starts loading a document again, or is gone. */
  reloading(): void {
    this.#loaded = this.#sent = this.#connected = false;
  }

  #deliver() {
    if (this.#target && this.#loaded && !this.#sent) {
      this.#send(this.#target);
      this.#sent = true;
    }
    if (this.#sent && this.#connected) {
      this.#target = undefined;
    }
  }
}
