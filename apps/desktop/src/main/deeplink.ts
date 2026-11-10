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
 * Hands each target to `deliver` once the app is ready; the ones that come
 * before, such as a link that started the app, wait for it.
 */
export class Navigation {
  readonly #deliver: (target: NavigateTarget) => void;
  #waiting: NavigateTarget[] | undefined = [];

  constructor(deliver: (target: NavigateTarget) => void) {
    this.#deliver = deliver;
  }

  go(target: NavigateTarget): void {
    if (this.#waiting) {
      this.#waiting.push(target);
    } else {
      this.#deliver(target);
    }
  }

  /** The app is ready: the waiting targets go, in order, and later ones at once. */
  ready(): void {
    const waiting = this.#waiting ?? [];
    this.#waiting = undefined;
    waiting.forEach((target) => this.#deliver(target));
  }
}
