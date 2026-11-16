import type { ProjectUpserted, SessionSummary } from "@cupper/protocol";
import { describe, expect, test } from "vitest";

import { nextNeedsAction, sidebarOrder, sidebarStep } from "./order";

const project = (project_id: string, position: number) =>
  ({ project_id, position }) as ProjectUpserted;
const session = (n: number, project_id: string, status = "idle") =>
  ({ session_id: `s_${n}`, project_id, status, last_activity_at: 100 - n }) as SessionSummary;

// p_2 comes first; p_1 has seven sessions, two of them behind "展开显示".
const projects = [project("p_1", 1), project("p_2", 0)];
const sessions = [
  session(9, "p_2"),
  ...[1, 2, 3, 4, 5, 6, 7].map((n) => session(n, "p_1", n === 7 ? "needs_action" : "idle")),
  session(8, "p_gone"),
];
const ids = (list: { session_id: string }[]) => list.map((s) => s.session_id);

describe("UNIT-ui-sidebar", () => {
  test("the order is the sidebar's: projects by position, sessions most recent first", () => {
    const order = sidebarOrder(projects, sessions, {});
    expect(ids(order.map((entry) => entry.session))).toEqual([
      "s_9",
      "s_1",
      "s_2",
      "s_3",
      "s_4",
      "s_5",
      "s_6",
      "s_7",
    ]);
    expect(order.filter((entry) => !entry.shown).map((entry) => entry.session.session_id)).toEqual([
      "s_6",
      "s_7",
    ]);
    expect(sidebarOrder(projects, sessions, { p_1: true }).every((entry) => entry.shown)).toBe(
      true,
    );
  });

  test("the sidebar's next and previous rows stop at the ends and skip hidden rows", () => {
    const order = sidebarOrder(projects, sessions, {});
    expect(sidebarStep(order, undefined, 1)?.session_id).toBe("s_9");
    expect(sidebarStep(order, undefined, -1)?.session_id).toBe("s_5");
    expect(sidebarStep(order, "s_9", 1)?.session_id).toBe("s_1");
    expect(sidebarStep(order, "s_9", -1)).toBeUndefined();
    expect(sidebarStep(order, "s_5", 1)).toBeUndefined();
  });

  test("the next session that needs action goes round, hidden rows too, and not to itself", () => {
    const order = sidebarOrder(projects, sessions, {});
    expect(nextNeedsAction(order, "s_1")).toMatchObject({
      session: { session_id: "s_7" },
      shown: false,
    });
    expect(nextNeedsAction(order, "s_7")).toBeUndefined();
    const two = sidebarOrder(projects, [...sessions, session(0, "p_2", "needs_action")], {});
    expect(nextNeedsAction(two, "s_7")?.session.session_id).toBe("s_0");
    expect(nextNeedsAction(two, undefined)?.session.session_id).toBe("s_0");
    expect(nextNeedsAction(two, "s_0")?.session.session_id).toBe("s_7");
  });
});
