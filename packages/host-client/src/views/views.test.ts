import type { EventBody } from "@cupper/protocol";
import { describe, expect, test } from "vitest";

import { StreamSync } from "../sync";
import { IndexView } from "./index-view";
import { SessionView } from "./session-view";
import { ViewError } from "./view";

const body = (type: string, data: object) => ({ type, data }) as EventBody;
const turn = (n: number) =>
  body("turn.started", { turn_id: `t_${n}`, turn_no: n, origin: "user", started_at: n });
const item = (id: string, kind: string, more: object = {}) =>
  body("item.started", { item_id: id, turn_id: "t_1", started_at: 1, kind, ...more });
const late = (id: string, of: string) =>
  item(id, "late_tool_output", {
    ref: { item_id: of, turn_id: "t_1", title: "Run" },
    output_state: "none",
  });

const TOOL = {
  tool_kind: "execute",
  title: "Run",
  paths: [],
  output_state: "none",
  extraction: "full",
};

describe("views", () => {
  test("versions change by key, and listeners hear of them once per flush", () => {
    const view = new SessionView();
    const heard: string[] = [];
    const off = view.versions.subscribe("item:i_1", () => heard.push("i_1"));
    view.versions.subscribe("turns", () => heard.push("turns"));
    const before = view.versions.of("view");
    view.apply(turn(1));
    view.apply(item("i_1", "tool_call", TOOL));
    view.apply(body("item.delta", { item_id: "i_1", field: "output", text: "a" }));
    expect([view.versions.of("item:i_2"), view.versions.of("view") > before]).toEqual([0, true]);
    expect([...view.versions.flush()].sort()).toEqual(["item:i_1", "turn:t_1", "turns", "view"]);
    expect(heard).toEqual(["turns", "i_1"]);
    // A late output changes the row of the tool call it reports on.
    off();
    const tool = view.versions.of("item:i_1");
    view.apply(late("i_2", "i_1"));
    expect(view.versions.of("item:i_1")).toBeGreaterThan(tool);
    expect(view.versions.flush()).toContain("item:i_2");
    expect(heard).toEqual(["turns", "i_1"]);
  });

  test("the sink: reset drops the view, live completes it, failed keeps the error", () => {
    const view = new IndexView();
    const project = { project_id: "p_1", name: "x", path: "/w", is_git: false, added_at: 1 };
    view.apply(body("project.upserted", { ...project, position: 0, trust_state: "trusted" }));
    view.live();
    const error = { code: -32000, message: "not_found" };
    view.failed(error);
    expect([view.complete, view.error, Object.keys(view.toJSON().projects)]).toEqual([
      true,
      error,
      ["p_1"],
    ]);
    const projects = view.versions.of("project:p_1");
    view.reset();
    expect([view.complete, view.toJSON()]).toEqual([
      false,
      { projects: {}, sessions: {}, elevations: {} },
    ]);
    expect(view.versions.of("project:p_1")).toBeGreaterThan(projects);
    view.live();
    expect([view.complete, view.error]).toEqual([true, undefined]);
  });

  test("unknown and malformed bodies wait as placeholders and change nothing", () => {
    const view = new SessionView();
    const unknown = body("canvas.drawn", { id: 1 });
    const event = { ...unknown, seq: 4, v: 1, ts: 1 };
    view.apply(turn(1));
    const json = JSON.stringify(view);
    view.apply(unknown, event);
    view.apply(body("turn.completed", { turn_id: "t_1" }));
    expect(JSON.stringify(view)).toBe(json);
    expect(view.unknown).toEqual([
      { body: unknown, event },
      { body: body("turn.completed", { turn_id: "t_1" }) },
    ]);
  });

  test("a body that does not fit throws a ViewError and changes nothing", () => {
    const view = new SessionView();
    view.apply(turn(1));
    const json = JSON.stringify(view);
    for (const bad of [
      turn(1),
      body("item.delta", { item_id: "i_9", field: "text", text: "x" }),
      body("interaction.resolved", { request_id: "r_9", status: "resolved" }),
      item("i_1", "thought", { turn_id: "t_9" }),
      body("project.removed", { project_id: "p_1" }),
    ]) {
      expect(() => view.apply(bad)).toThrow(ViewError);
      expect(JSON.stringify(view)).toBe(json);
    }
    expect(() => new IndexView().apply(turn(1))).toThrow("belongs to the other stream");
  });

  test("as a StreamSync's sink, a ViewError starts the stream over", () => {
    const view = new SessionView();
    const counters = { gaps: 0, duplicates: 0, resets: 0 };
    const sync = new StreamSync("session/s_1", view, counters, () => undefined);
    sync.subscribe();
    sync.subscribed({ stream: "session/s_1", mode: "snapshot", at_seq: 1, epoch: 1, head_seq: 1 });
    sync.snapshot({
      stream: "session/s_1",
      at_seq: 1,
      epoch: 1,
      page: 0,
      last: true,
      ops: [turn(1)],
    });
    expect([view.complete, view.toJSON().turns.length]).toEqual([true, 1]);
    const delta = {
      ...body("item.delta", { item_id: "i_9", field: "text", text: "x" }),
      seq: 2,
      v: 1,
      ts: 1,
    };
    expect(sync.events({ stream: "session/s_1", events: [delta] })).toEqual({
      stream: "session/s_1",
    });
    expect([view.complete, view.toJSON().turns, counters.resets]).toEqual([false, [], 1]);
  });
});
