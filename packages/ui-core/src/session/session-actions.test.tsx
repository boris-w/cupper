// @vitest-environment jsdom
import { hostError, type MockHost, type Reply } from "@cupper/host-client/mock";
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeAll, describe, expect, test } from "vitest";

import type { Route } from "../app/router";
import { mountOnHost } from "../review/test-harness";
import { useSessionActions } from "./actions";

// jsdom has no modal dialogs.
beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function () {
    this.open = false;
  };
});
afterEach(cleanup);

const PROJECT = {
  type: "project.upserted",
  data: { project_id: "p_1", name: "cupper", position: 0, trust_state: "trusted" },
};
const summary = (n: number, workspace_kind: string, title: string) => ({
  type: "session.upserted",
  data: {
    ...{ session_id: `s_${n}`, title, status: "idle", agent: "claude", project_id: "p_1" },
    ...{ title_source: "prompt", last_activity_at: 1, pending_interactions: 0 },
    ...{ read_only: false, unread: false, workspace_kind },
  },
});
/** s_1 works in the project, s_2 in a worktree. */
const INDEX = [PROJECT, summary(1, "local", "修登录"), summary(2, "worktree", "加缓存")];

/** `node` on a host whose index is `INDEX` and whose answers `replies` gives; the window shows `route`. */
export async function setup(
  node: ReactNode = null,
  replies: Record<string, Reply> = {},
  route?: Route,
) {
  const handle = (mock: MockHost) => {
    for (const [method, reply] of Object.entries(replies)) mock.handle(method, () => reply);
  };
  const { mock, ui, controls } = await mountOnHost(node, handle, { index: INDEX, route });
  await settle();
  const calls = (method: string) =>
    mock.requests.filter((r) => r.method === method).map((r) => r.params);
  return { controls, ui, calls };
}

/** Lets the host's answers arrive. */
export const settle = () =>
  act(() => new Promise((done) => requestAnimationFrame(() => done(undefined))));
export const toasts = () => screen.queryAllByRole("status").map((toast) => toast.textContent);
const click = (name: string) => fireEvent.click(screen.getByRole("button", { name }));
const dialog = () => screen.queryByRole("dialog");

const ARCHIVED = { result: { worktree: "kept" } };
const check = (removable: boolean, more: object = {}) => ({
  result: {
    removable,
    blockers: [],
    warnings: { ignored_files: 0, case_only_dirty: false },
    ...more,
  },
});

describe("session actions", () => {
  test("rename asks for the title in a dialog, as the menu's command on the session shown", async () => {
    const { controls, calls } = await setup(null, { "session/rename": { result: {} } });
    act(() => controls.command({ id: "rename" }));
    const input = screen.getByRole("textbox", { name: "会话标题" });
    expect(input).toHaveProperty("value", "修登录");
    fireEvent.change(input, { target: { value: "  " } });
    expect(screen.getByRole("button", { name: "保存" })).toHaveProperty("disabled", true);
    fireEvent.change(input, { target: { value: " 修登录的 bug " } });
    fireEvent.submit(input);
    await settle();
    expect(calls("session/rename")).toEqual([
      expect.objectContaining({ session_id: "s_1", title: "修登录的 bug" }),
    ]);
    expect(dialog()).toBeNull();
  });

  test("a local session archives at once; the window leaves it", async () => {
    const { controls, ui, calls } = await setup(null, { "session/archive": ARCHIVED });
    act(() => controls.command({ id: "archive" }));
    await settle();
    expect(calls("worktree/check")).toEqual([]);
    expect(calls("session/archive")).toEqual([
      expect.objectContaining({ session_id: "s_1", worktree: "keep" }),
    ]);
    expect(ui.getState().route).toEqual({ page: "new" });
    expect(toasts()).toEqual(["已归档「修登录」"]);
  });

  test("a worktree with nothing to lose: the user picks whether it goes too", async () => {
    const removed = { "session/archive": { result: { worktree: "removed" } } };
    const route: Route = { page: "session", sessionId: "s_2" };
    const ignored = check(true, { warnings: { ignored_files: 3, case_only_dirty: false } });
    const { controls, ui, calls } = await setup(
      null,
      { "worktree/check": ignored, ...removed },
      route,
    );
    act(() => controls.command({ id: "archive" }));
    await settle();
    expect(calls("worktree/check")).toEqual([{ session_id: "s_2" }]);
    expect(dialog()?.textContent).toContain("同时删除这个会话的 worktree？");
    expect(dialog()?.textContent).toContain("其中被忽略的 3 个文件会一并删除。");
    click("删除 worktree");
    await settle();
    expect(calls("session/archive")).toEqual([
      expect.objectContaining({ session_id: "s_2", worktree: "remove" }),
    ]);
    expect(dialog()).toBeNull();
    expect(ui.getState().route).toEqual({ page: "new" });
    expect(toasts()).toEqual(["已归档「加缓存」，worktree 已删除"]);
  });

  test("files changed only in case take the user's word to remove", async () => {
    const caseOnly = check(true, { warnings: { ignored_files: 0, case_only_dirty: true } });
    const { calls } = await setup(<Archive id="s_2" />, {
      "worktree/check": caseOnly,
      "session/archive": { result: { worktree: "removed" } },
    });
    click("archive s_2");
    await settle();
    expect(dialog()?.textContent).toContain("只因路径的大小写不同");
    click("删除 worktree");
    await settle();
    expect(calls("session/archive")).toEqual([
      expect.objectContaining({ worktree: "remove_case_only_dirty" }),
    ]);
  });

  test("a worktree that would lose work stays: the dialog says why, the toast too", async () => {
    const blockers = ["modified_files", "something_new"];
    const blocked = { result: { worktree: "blocked", blockers: ["unpushed_commits"] } };
    const { calls } = await setup(<Archive id="s_2" />, {
      "worktree/check": check(false, { blockers }),
      "session/archive": blocked,
    });
    click("archive s_2");
    await settle();
    const items = screen.getAllByRole("listitem").map((item) => item.textContent);
    expect(items).toEqual(["有已修改或已暂存的文件", "其他原因（something_new）"]);
    expect(screen.queryByRole("button", { name: "删除 worktree" })).toBeNull();
    click("归档");
    await settle();
    expect(calls("session/archive")).toEqual([expect.objectContaining({ worktree: "keep" })]);
    expect(toasts()).toEqual(["已归档「加缓存」，worktree 没有删除：有还没推送的提交"]);
  });

  test("a refusal says why; Esc or 取消 archives nothing", async () => {
    const { controls, ui } = await setup(null, {
      "session/archive": hostError("conflict", "turn_running"),
    });
    act(() => controls.command({ id: "archive" }));
    await settle();
    expect(toasts()).toEqual(["有回合正在运行，先停下回合再试"]);
    expect(ui.getState().route).toEqual({ page: "session", sessionId: "s_1" });

    act(() => controls.command({ id: "rename" }));
    fireEvent(dialog() as HTMLElement, new Event("cancel", { cancelable: true }));
    expect(dialog()).toBeNull();
  });

  test("the menu's commands act only on a session page", async () => {
    const { controls, calls } = await setup(null, {}, { page: "new" });
    act(() => {
      for (const id of ["rename", "archive", "open_in_terminal"] as const) controls.command({ id });
    });
    await settle();
    expect(dialog()).toBeNull();
    expect(calls("session/archive")).toEqual([]);
    expect(calls("workspace/open")).toEqual([]);
  });

  test("⌘⌥T opens the workspace in the terminal", async () => {
    const { controls, calls } = await setup(null, { "workspace/open": { result: {} } });
    act(() => controls.command({ id: "open_in_terminal" }));
    await settle();
    expect(calls("workspace/open")).toEqual([{ session_id: "s_1", target: "terminal" }]);
  });
});

/** A button that archives `id`, as the sidebar's menu does. */
function Archive({ id }: { id: string }) {
  const { archive } = useSessionActions();
  return (
    <button type="button" onClick={() => archive(id)}>
      archive {id}
    </button>
  );
}
