import { describe, expect, test } from "vitest";

import { compare, FILES, render, reviewed, summarize, type Node, type Schemas } from "./drift.ts";

const ref = (name: string) => ({ $ref: `#/definitions/${name}` });
const TurnParams = (properties: string[]): Node =>
  ({
    type: "object",
    properties: Object.fromEntries(
      properties.map((name) => [name, name === "mode" ? ref("Mode") : {}]),
    ),
    required: ["threadId"],
  }) as Node;

/** codex's schema files with `requests` and `notifications`, `[method, params]` each. */
function schemas(
  requests: [string, Node | null][],
  notifications: [string, Node][] = [],
  defs: Record<string, Node> = {},
) {
  const variants = (list: [string, Node | null][]) =>
    list.map(([method, params]) => ({
      properties: { method: { enum: [method] }, ...(params && { params }) },
    }));
  const definitions = { Mode: { enum: ["default", "plan"] }, ...defs };
  return {
    [FILES.request]: { oneOf: variants(requests), definitions },
    [FILES.notification]: { oneOf: variants(notifications), definitions },
  } as Schemas;
}

const ADAPTER = `client.request("turn/start", p); on("turn/completed"); client.request("thread/list");`;
const THREAD_LIST = { anyOf: [ref("ListParams"), { type: "null" }] } as Node;
const base = (params = TurnParams(["threadId", "mode"]), Done: Node = {}) =>
  schemas(
    [
      ["turn/start", ref("TurnStartParams")],
      ["thread/list", THREAD_LIST],
      ["unused", null],
    ],
    [["turn/completed", ref("Done")]],
    { TurnStartParams: params, ListParams: TurnParams(["cursor"]), Done },
  );
const BASELINE = summarize(base(), base(), ADAPTER);

describe("UNIT-codex-drift", () => {
  test("the summary holds the methods the adapter names, and what their params reach", () => {
    expect(Object.keys(BASELINE.methods)).toEqual(["thread/list", "turn/completed", "turn/start"]);
    expect(BASELINE.methods["turn/start"]).toMatchObject({
      kind: "request",
      stable: true,
      defs: ["Mode", "TurnStartParams"],
    });
    expect(BASELINE.methods["thread/list"]?.defs).toEqual(["ListParams"]);
    expect(BASELINE.methods["turn/completed"]?.kind).toBe("notification");
    expect(BASELINE.defs["Mode"]?.facets).toEqual(["enum default", "enum plan"]);
  });

  test("the same schema is no change", () => {
    expect(compare(BASELINE, summarize(base(), base(), ADAPTER))).toEqual([]);
  });

  test("a method gone", () => {
    const now = base();
    now[FILES.notification] = { oneOf: [] };
    expect(compare(BASELINE, summarize(now, now, ADAPTER))).toEqual([
      { method: "turn/completed", change: "gone" },
    ]);
  });

  test("a stable method that is experimental only", () => {
    const stable = schemas(
      [["turn/start", ref("TurnStartParams")]],
      [["turn/completed", ref("Done")]],
      { TurnStartParams: TurnParams(["threadId", "mode"]), Done: {} },
    );
    expect(compare(BASELINE, summarize(base(), stable, ADAPTER))).toEqual([
      { method: "thread/list", change: "experimental" },
    ]);
  });

  test("params with a field added and one removed, down to the definitions they reach", () => {
    const now = base(TurnParams(["threadId", "parentTurnId"]));
    expect(compare(BASELINE, summarize(now, now, ADAPTER))).toEqual([
      {
        method: "turn/start",
        change: "params",
        defs: [
          "Mode removed -enum default -enum plan",
          "TurnStartParams changed +property parentTurnId -property mode",
        ],
      },
    ]);
    const required = base({
      ...TurnParams(["threadId", "mode"]),
      required: ["threadId", "mode"],
    } as Node);
    const [change] = compare(BASELINE, summarize(required, required, ADAPTER));
    expect(change?.defs).toEqual(["TurnStartParams changed +required mode"]);
  });

  test("a field gone from one variant of a tagged union", () => {
    const variant = (type: string, ...fields: string[]) => ({
      properties: Object.fromEntries([["type", { enum: [type] }], ...fields.map((f) => [f, {}])]),
    });
    const item = (...image: string[]) => ({
      oneOf: [variant("image", ...image), variant("tool", "result")],
    });
    const [was, now] = [base(undefined, item("result")), base(undefined, item())];
    const [change] = compare(summarize(was, was, ADAPTER), summarize(now, now, ADAPTER));
    expect(change?.defs).toEqual(["Done changed -image: property result"]);
  });

  test("a drift reviewed passes while it is the same, against the same baseline", () => {
    const now = base(TurnParams(["threadId", "mode", "parentTurnId"]));
    const drift = render(compare(BASELINE, summarize(now, now, ADAPTER)));
    expect(drift).toEqual([
      "turn/start: params changed",
      "  TurnStartParams changed +property parentTurnId",
    ]);
    const review = {
      baseline: "0.163.0",
      reviewed: "2026-10-28",
      task: "T113",
      verdict: "ok",
      drift,
    };
    expect(reviewed(drift, "0.163.0", review)).toBe(true);
    expect(reviewed(drift, "0.163.0")).toBe(false);
    expect(reviewed(drift, "0.165.0", review)).toBe(false);
    expect(reviewed([...drift, "thread/list: gone"], "0.163.0", review)).toBe(false);
  });

  test("a method the adapter uses that the baseline does not have", () => {
    const now = base();
    now[FILES.request]?.oneOf?.push({ properties: { method: { enum: ["thread/fork"] } } });
    expect(compare(BASELINE, summarize(now, now, `${ADAPTER} request("thread/fork")`))).toEqual([
      { method: "thread/fork", change: "new" },
    ]);
  });
});
