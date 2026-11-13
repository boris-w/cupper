// deeplink (design 5.7.2, 5.7.8): only the one route of this build kind's
// scheme, with well-formed IDs, navigates; the window gets the target once
// it has a document, and again after each reload until it connects.

import { describe, expect, test } from "vitest";

import { deepLink, SCHEMES, sessionTarget, WindowNavigation } from "./deeplink";

const session = { kind: "session", session_id: "s_0190a3b4" } as const;

describe("deeplink", () => {
  test("the one route, with and without a request", () => {
    expect(deepLink("cupper-local://session/s_0190a3b4", "cupper-local")).toEqual(session);
    expect(deepLink("cupper://session/s_0190a3b4?request=r_7-x", "cupper")).toEqual({
      ...session,
      request_id: "r_7-x",
    });
    expect(Object.values(SCHEMES)).toEqual(["cupper", "cupper-local", "cupper-e2e", "cupper-dev"]);
  });

  test("anything else is ignored", () => {
    const ignored = [
      // Another build kind's scheme, or the generic one.
      "cupper://session/s_0190a3b4",
      "cupper-dev://session/s_0190a3b4",
      // Another route, more path, IDs of the wrong kind or form.
      "cupper-local://settings/s_0190a3b4",
      "cupper-local://session/s_0190a3b4/delete",
      "cupper-local://session/",
      "cupper-local://session/p_0190a3b4",
      "cupper-local://session/s_" + "a".repeat(65),
      "cupper-local://session/s_%2e%2e",
      "cupper-local://session/s_1?request=s_2",
      "cupper-local://session/s_1?request=",
      // More than the route says.
      "cupper-local://session/s_1?request=r_2&option=allow",
      "cupper-local://session/s_1?approve=1",
      "cupper-local://session/s_1#panel",
      "cupper-local://user@session/s_1",
      "cupper-local://session:8080/s_1",
      "not a url",
    ];
    for (const url of ignored) {
      expect(deepLink(url, "cupper-local"), url).toBeNull();
    }
    expect(sessionTarget("s_1", 7)).toBeNull();
    expect(sessionTarget(undefined)).toBeNull();
  });

  test("a window gets the target on each load until a document has its connection", () => {
    const sent: unknown[] = [];
    const window = new WindowNavigation((target) => sent.push(target));
    window.go(session);
    expect(sent).toEqual([]);
    window.loaded();
    // The host restarts: the connection failed, and the window loads again.
    window.reloading();
    window.loaded();
    expect(sent).toEqual([session, session]);
    window.connected();
    window.reloading();
    window.loaded();
    expect(sent).toHaveLength(2);
    // A newer target replaces an older one; a connection before the load counts once it is sent.
    window.go({ kind: "session", session_id: "s_old" });
    window.reloading();
    window.go(session);
    window.connected();
    window.loaded();
    window.reloading();
    window.loaded();
    expect(sent).toEqual([session, session, { kind: "session", session_id: "s_old" }, session]);
  });
});
