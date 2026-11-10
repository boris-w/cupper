// deeplink (design 5.7.2, 5.7.8): only the one route of this build kind's
// scheme, with well-formed IDs, navigates; and nothing goes before the app
// is ready.

import { describe, expect, test } from "vitest";

import { deepLink, Navigation, SCHEMES, sessionTarget } from "./deeplink";

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

  test("targets wait until the app is ready, then go in order", () => {
    const delivered: unknown[] = [];
    const navigation = new Navigation((target) => delivered.push(target));
    navigation.go(session);
    navigation.go({ ...session, request_id: "r_1" });
    expect(delivered).toEqual([]);
    navigation.ready();
    navigation.go({ kind: "session", session_id: "s_2" });
    expect(delivered).toEqual([
      session,
      { ...session, request_id: "r_1" },
      { kind: "session", session_id: "s_2" },
    ]);
  });
});
