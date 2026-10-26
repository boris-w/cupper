import { describe, expect, test } from "vitest";

import { diff, empty, shape, type Kind } from "./shape.ts";

type Message = Record<string, unknown>;

/** A recording: its header, then each `[dir, msg]` as a frame. */
const recording = (frames: [string, Message][], t = 0) =>
  [
    JSON.stringify({ format: "cupper-rec", v: 1, scenario: "s" }),
    ...frames.map(([dir, msg], n) =>
      JSON.stringify({ seq: n + 1, t: t + n, instance: 0, dir, msg: { jsonrpc: "2.0", ...msg } }),
    ),
    JSON.stringify({
      seq: 99,
      t: 99,
      instance: 0,
      dir: "note",
      note: { event: "instance_exited" },
    }),
  ].join("\n");

/** A turn of a session with its id and text, a permission with its options, the prompt's result. */
function turn({ id = 1, text = "hi", session = "s-1", options = ["allow-once", "reject"] } = {}) {
  const update = (sessionUpdate: string, more: Message = {}) => ({
    method: "session/update",
    params: { sessionId: session, update: { sessionUpdate, ...more } },
  });
  const ask = options.map((optionId) => ({
    optionId,
    kind: optionId.replace("-", "_"),
    name: optionId,
  }));
  return [
    [
      "send",
      {
        id,
        method: "session/prompt",
        params: { sessionId: session, prompt: [{ type: "text", text }] },
      },
    ],
    [
      "recv",
      update("agent_message_chunk", { content: { type: "text", text }, _meta: { model: text } }),
    ],
    [
      "recv",
      { id: 0, method: "session/request_permission", params: { sessionId: session, options: ask } },
    ],
    ["send", { id: 0, result: { outcome: { outcome: "selected", optionId: options[0] } } }],
    ["recv", { id, result: { stopReason: "end_turn", usage: { inputTokens: id } } }],
  ] as [string, Message][];
}

/** The kinds in which `now` differs from {@link turn}. */
const differs = (now: [string, Message][]) => {
  const found = diff(shape(recording(turn())), shape(recording(now)));
  return Object.entries(found)
    .filter(([, d]) => d.added.length || d.removed.length)
    .map(([kind, d]) => `${kind as Kind}: +${d.added.join(",")} -${d.removed.join(",")}`);
};

describe("UNIT-rec-shape-diff", () => {
  test("the same shape: text, times, IDs and sessions differ, nothing else", () => {
    const a = shape(recording(turn()));
    const b = shape(recording(turn({ id: 7, text: "other words", session: "s-9" }), 5_000));
    expect(empty(diff(a, b))).toBe(true);
    expect([...a.methods].sort()).toEqual([
      "recv session/request_permission",
      "recv session/update",
      "send session/prompt",
    ]);
    expect([...a.meta]).toEqual(["session/update params.update._meta.model"]);
  });

  test("each of the six kinds", () => {
    const base = turn();
    const add = (frame: [string, Message]) => [...base, frame];
    expect(differs(add(["send", { method: "session/cancel", params: {} }]))).toEqual([
      "methods: +send session/cancel -",
    ]);
    const plan = { method: "session/update", params: { update: { sessionUpdate: "plan" } } };
    expect(differs(add(["recv", plan]))).toEqual(["updates: +plan -"]);
    const meta = {
      method: "session/update",
      params: { update: { sessionUpdate: "plan", _meta: { a: { b: [{ c: 1 }] } } } },
    };
    expect(differs(add(["recv", meta]))).toEqual([
      "updates: +plan -",
      "meta: +session/update params.update._meta.a,session/update params.update._meta.a.b,session/update params.update._meta.a.b[].c -",
    ]);
    expect(differs(turn({ options: ["allow-once", "allow-always"] }))).toEqual([
      "options: +allow-always (allow_always) -reject (reject)",
    ]);
    const stop = base.map(([dir, msg]) => [
      dir,
      msg.result && "stopReason" in (msg.result as Message)
        ? { ...msg, result: { stopReason: "cancelled" } }
        : msg,
    ]) as [string, Message][];
    expect(differs(stop)).toEqual(["stops: +cancelled -end_turn"]);
    const error = add(["send", { id: 5, method: "session/load", params: {} }]);
    error.push(["recv", { id: 5, error: { code: -32000, message: "auth required" } }]);
    expect(differs(error)).toEqual([
      "methods: +send session/load -",
      "errors: +session/load -32000 -",
    ]);
  });
});
