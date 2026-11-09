// @vitest-environment jsdom
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { mountOnHost } from "../review/test-harness";
import { StreamActions } from "../stream/actions";
import { RowView } from "./registry";
import { streamRows, type RowOf } from "./stream";
import "./changes";

afterEach(cleanup);

const ROW: RowOf<"turn_changes"> = {
  ...{ kind: "turn_changes", key: "changes:t_1", turnId: "t_1", turnNo: 1 },
  snapshot: {
    turn_id: "t_1",
    phase: "end",
    status: "ok",
    stat: { files: 1, added: 1, removed: 0 },
    excluded: [],
  },
};
const wait = (ms: number) => act(() => new Promise((done) => setTimeout(done, ms)));

describe("UI-turn-changes", () => {
  test("undo asks for a fresh token: the one on the card may have expired", async () => {
    let tokens = 0;
    const file = { path: "README.md", additions: 1, deletions: 0, binary: false, change: "update" };
    const noop = () => undefined;
    const { mock } = await mountOnHost(
      <StreamActions
        value={{ sessionId: "s_1", loadEarlier: noop, toggleTurn: noop, jumpTo: noop }}
      >
        <RowView registry={streamRows} row={ROW} />
      </StreamActions>,
      (mock) => {
        mock.handle("git/diff", () => {
          const diff_token = ["dt_A", "dt_B"][tokens++] ?? "dt_C";
          return {
            result: {
              caveats: [],
              diff_token,
              files: [file],
              stat: { added: 1, removed: 0, files: 1 },
            },
          };
        });
        mock.handle("git/revert", () => ({ result: {} }));
      },
    );
    expect(await screen.findByText("README.md")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "撤销" }));
    const confirm = await screen.findByRole("button", { name: "确认" });
    // Armed after 500 ms; then a key on "确认", where the focus is.
    await wait(550);
    expect(document.activeElement).toBe(confirm);
    fireEvent.click(confirm);
    await wait(50);
    const reverts = mock.requests.filter((r) => r.method === "git/revert").map((r) => r.params);
    expect(reverts).toEqual([
      expect.objectContaining({ diff_token: "dt_B", target: { all: true } }),
    ]);
  });
});
