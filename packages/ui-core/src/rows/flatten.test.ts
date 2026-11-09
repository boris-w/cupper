import type { Placeholder, SessionView } from "@cupper/host-client";
import { describe, expect, test } from "vitest";

import { flatten } from "./flatten";
import type { StreamRow } from "./stream";

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
      "item:u",
      "turn:t_7",
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
      "user_message",
      "turn_header",
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
    expect(of("turn:t_7")).toMatchObject({ status: "cancelled", turnNo: 7, durationMs: 4000 });
    // A cancelled turn stays open, as a running one does.
    expect(of("turn:t_7")).not.toHaveProperty("folded");
    expect(of("turn:t_8")).not.toHaveProperty("folded");
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

  test("a completed turn folds to its header, its final answer and what always shows", () => {
    const ended = (turn_id: string, status: string, final_item_ids: string[]) => ({
      ...{ turn_id, status, duration_ms: 4000, ended_at: 14, final_item_ids },
    });
    const ref = { item_id: "c0", turn_id: "t_8", title: "cargo test", command: "cargo test" };
    const end = (turn_id: string, status: string, more: object = {}) => ({
      ...{ turn_id, phase: "end", status, excluded: [] },
      ...more,
    });
    const view = {
      turns: [
        {
          started: { turn_id: "t_8", turn_no: 8, started_at: 1, origin: "user" },
          completed: ended("t_8", "cancelled", []),
          entries: entries("c0"),
          // No card in a folder that is no git repository.
          snapshots: [end("t_8", "skipped", { reason: "not_a_repo" })],
        },
        {
          started: { turn_id: "t_9", turn_no: 9, started_at: 20, origin: "user" },
          completed: ended("t_9", "completed", ["f"]),
          entries: entries("u", "th", "c1", "st", "p", "l", "m", "c2", "f", "r2"),
          snapshots: [end("t_9", "ok", { stat: { files: 2, added: 3, removed: 1 } })],
        },
      ],
      tail: [],
      items: {
        c0: tool("c0"),
        u: item("u", "user_message", { content: [], steered: false }),
        th: item("th", "thought"),
        c1: tool("c1"),
        st: item("st", "user_message", { content: [], steered: true }),
        p: item("p", "plan", { source: "entries", entries: [] }),
        l: item("l", "late_tool_output", { ref, output_state: "complete", exit_code: 0 }),
        m: item("m", "agent_message"),
        c2: tool("c2"),
        f: item("f", "agent_message"),
      },
      requests: {},
      detached: [],
      late_by_ref: { c0: "l" },
    } as unknown as ViewJson;
    const keys = (rows: StreamRow[]) => rows.map((row) => row.key);
    const folded = flatten(view, [], "none");
    expect(keys(folded)).toEqual([
      ...["turn:t_8", "item:c0", "item:u", "turn:t_9"],
      ...["item:st", "item:p", "item:l", "item:f", "request:r2", "changes:t_9"],
    ]);
    expect(folded.find((row) => row.key === "turn:t_9")).toMatchObject({ folded: true });
    expect(folded.at(-1)).toMatchObject({ kind: "turn_changes", turnId: "t_9", turnNo: 9 });
    // The call whose output came late carries its final exit code.
    expect(folded[1]).toMatchObject({ kind: "tool", late: { itemId: "l", exitCode: 0 } });
    const open = flatten(view, [], "none", new Set(["t_9"]));
    expect(keys(open)).toEqual([
      ...["turn:t_8", "item:c0", "item:u", "turn:t_9", "item:th", "item:c1", "item:st", "item:p"],
      ...["item:l", "item:m", "item:c2", "item:f", "request:r2", "changes:t_9"],
    ]);
    expect(open.find((row) => row.key === "turn:t_9")).toMatchObject({ folded: false });
    // Without a final answer, the folded turn shows its header and what always shows.
    // A turn whose changes could not be recorded still has its card.
    const silent = {
      ...view,
      turns: [
        {
          ...view.turns[1],
          completed: ended("t_9", "completed", []),
          snapshots: [end("t_9", "failed", { reason: "timeout" })],
        },
      ],
    };
    expect(keys(flatten(silent as ViewJson, [], "none"))).toEqual([
      ...["item:u", "turn:t_9", "item:st", "item:p", "item:l", "request:r2", "changes:t_9"],
    ]);
  });
});
