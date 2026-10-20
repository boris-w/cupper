import type { Placeholder, SessionView } from "@cupper/host-client";
import { describe, expect, test } from "vitest";

import { flatten } from "./flatten";

type ViewJson = ReturnType<SessionView["toJSON"]>;

const item = (item_id: string, kind: string, extra: object = {}, ended = true) => ({
  item: { item_id, kind, started_at: 1, ...extra },
  text: `${item_id} text`,
  output: "",
  ...(ended ? { end: { status: "completed", duration_ms: 1500, ended_at: 2 } } : {}),
});
const tool = (id: string, group_id?: string) =>
  item(id, "tool_call", {
    title: id,
    tool_kind: "execute",
    paths: [],
    output_state: "none",
    ...(group_id ? { group_id } : {}),
  });
const entries = (...ids: string[]) =>
  ids.map((id) => (id.startsWith("r") ? { request: id } : { item: id }));

const VIEW = {
  turns: [
    {
      started: { turn_id: "t_7", turn_no: 7, started_at: 10, origin: "user" },
      completed: {
        turn_id: "t_7",
        status: "cancelled",
        duration_ms: 4000,
        ended_at: 14,
        final_item_ids: [],
      },
      entries: entries("u", "a", "g1", "g2", "h", "n", "g3", "r1", "x"),
      snapshots: [],
    },
    {
      started: { turn_id: "t_8", turn_no: 8, started_at: 20, origin: "user" },
      entries: entries("s"),
      snapshots: [],
    },
  ],
  tail: entries("e"),
  items: {
    u: item("u", "user_message", { content: [{ type: "text", text: "hi" }], steered: false }),
    a: item("a", "agent_message", {}, false),
    g1: tool("g1", "g"),
    g2: tool("g2", "g"),
    h: tool("h"),
    n: item("n", "notice", { code: "compaction", params: {} }),
    g3: tool("g3", "g"),
    x: item("x", "future_kind"),
    s: item("s", "thought", {}, false),
    e: item("e", "error", { code: "process_exited", guidance: { kind: "none" }, retryable: false }),
  },
  requests: {},
  detached: [],
  late_by_ref: {},
} as unknown as ViewJson;

describe("UNIT-stream-rows", () => {
  test("turns, their entries, the tail and the unknown, one row each", () => {
    const unknown = [{ body: { type: "future.event", data: {} } }] as Placeholder[];
    const rows = flatten(VIEW, unknown, "more");
    expect(rows.map((row) => row.key)).toEqual([
      "history",
      "turn:t_7",
      "item:u",
      "item:a",
      "group:g1",
      "item:h",
      "item:n",
      "item:g3",
      "request:r1",
      "item:x",
      "turn:t_8",
      "item:s",
      "item:e",
      "unknown:0",
    ]);
    expect(rows.map((row) => row.kind)).toEqual([
      "history",
      "turn_header",
      "user_message",
      "agent_message",
      "activity_group",
      "tool",
      "notice",
      "tool",
      "request",
      "unknown",
      "turn_header",
      "thought",
      "error",
      "unknown",
    ]);
  });

  test("each row carries what it shows", () => {
    const rows = flatten(VIEW, [], "none");
    const of = (key: string) => rows.find((row) => row.key === key);
    expect(rows[0]).toMatchObject({
      kind: "turn_header",
      status: "cancelled",
      turnNo: 7,
      durationMs: 4000,
    });
    expect(of("turn:t_8")).toMatchObject({ status: "running", startedAt: 20 });
    expect(of("turn:t_8")).not.toHaveProperty("durationMs");
    expect(of("item:u")).toMatchObject({ content: [{ type: "text", text: "hi" }], steered: false });
    expect(of("item:a")).toMatchObject({ text: "a text", streaming: true });
    expect(of("item:s")).toMatchObject({ kind: "thought", running: true });
    expect(of("group:g1")).toMatchObject({ groupId: "g", running: false });
    const group = of("group:g1") as { tools: { itemId: string; call: object }[] };
    expect(group.tools.map((tool) => tool.itemId)).toEqual(["g1", "g2"]);
    expect(group.tools[0]?.call).toEqual({
      title: "g1",
      tool_kind: "execute",
      paths: [],
      output_state: "none",
    });
    expect(of("item:n")).toMatchObject({ notice: { code: "compaction", params: {} } });
    expect(of("item:x")).toMatchObject({ kind: "unknown", type: "future_kind" });
  });
});
