// @vitest-environment jsdom
import { hostError, type Reply } from "@cupper/host-client/mock";
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, test } from "vitest";

import { GitActions } from "./GitActions";
import { mountOnHost } from "./test-harness";

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

const BRANCH = { branch: "cupper/fix-login", upstream: "origin/cupper/fix-login" };

async function setup(
  status: object = BRANCH,
  reply: (method: string) => Reply | undefined = () => undefined,
) {
  const { mock } = await mountOnHost(<GitActions sessionId="s_1" />, (mock) => {
    mock.handle("git/status", () => ({ result: { ahead: 0, behind: 0, files: [], ...status } }));
    const files = [{ path: "a.rs", additions: 1, deletions: 0, binary: false, change: "update" }];
    const diff = {
      caveats: [],
      diff_token: "dt_u",
      files,
      stat: { added: 1, removed: 0, files: 1 },
    };
    mock.handle("git/diff", () => ({ result: diff }));
    for (const method of ["git/stage", "git/commit", "git/push"]) {
      mock.handle(
        method,
        () => reply(method) ?? { result: method === "git/commit" ? { oid: "ab" } : {} },
      );
    }
  });
  const settle = () => act(() => new Promise((done) => setTimeout(done, 0)));
  await settle();
  const sent = () =>
    mock.requests
      .filter((r) => /^git\/(diff|stage|commit|push)$/.test(r.method))
      .map((r) => [r.method, r.params as Record<string, unknown>] as const);
  const open = async (item: string) => {
    fireEvent.keyDown(screen.getByRole("button", { name: "Git 操作" }), { key: "Enter" });
    await settle();
    fireEvent.click(screen.getByRole("menuitem", { name: item }));
    await settle();
  };
  const submit = async (message?: string) => {
    if (message !== undefined) {
      fireEvent.change(screen.getByRole("textbox", { name: "提交信息" }), {
        target: { value: message },
      });
    }
    fireEvent.click(screen.getByRole("button", { name: "继续" }));
    await settle();
  };
  return { mock, sent, open, submit, settle };
}

describe("UNIT-review-commit", () => {
  test("commit, commit and push, push: the commands of each next step", async () => {
    const { mock, sent, open, submit } = await setup();
    await open("提交…");
    expect(screen.getByRole("dialog", { name: "提交或推送" })).toBeTruthy();
    expect(screen.getByText("提交到 cupper/fix-login")).toBeTruthy();
    expect(screen.getByText("将运行仓库的 hooks")).toBeTruthy();
    expect(screen.getByRole("button", { name: "继续" })).toHaveProperty("disabled", true);
    await submit("修登录");
    expect(sent()).toEqual([["git/commit", expect.objectContaining({ message: "修登录" })]]);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText("已提交")).toBeTruthy();

    mock.requests.length = 0;
    await open("提交…");
    fireEvent.click(screen.getByRole("radio", { name: "提交并推送" }));
    await submit("再修");
    expect(sent().map(([method]) => method)).toEqual(["git/commit", "git/push"]);
    expect(sent()[1]?.[1]).not.toHaveProperty("set_upstream");

    mock.requests.length = 0;
    await open("推送");
    expect(screen.getByRole("radio", { name: "推送" })).toHaveProperty("checked", true);
    expect(screen.getByRole("textbox", { name: "提交信息" })).toHaveProperty("disabled", true);
    await submit();
    expect(sent().map(([method]) => method)).toEqual(["git/push"]);
    expect(screen.getByText("已推送")).toBeTruthy();
  });

  test("包含未暂存的更改 stages on a fresh token first; a failed commit says they stay staged", async () => {
    const stderr = "<b>pre-commit</b>: lint failed";
    const { error } = hostError("git_error", "hook_failed");
    const failed = { error: { ...error, data: { ...error.data, code: "git_error", stderr } } };
    const { sent, open, submit } = await setup(BRANCH, (m) =>
      m === "git/commit" ? failed : undefined,
    );
    await open("提交…");
    fireEvent.click(screen.getByRole("checkbox", { name: "包含未暂存的更改" }));
    await submit("修登录");
    expect(sent()).toEqual([
      ["git/diff", { session_id: "s_1", scope: "unstaged" }],
      ["git/stage", expect.objectContaining({ diff_token: "dt_u", target: { all: true } })],
      ["git/commit", expect.objectContaining({ message: "修登录" })],
    ]);
    expect(screen.getByRole("alert").textContent).toContain(
      "仓库的 hook 拒绝了这次提交。 改动已在已暂存里。",
    );
    // git's output is text: its markup stays characters.
    expect(screen.getByText(stderr).tagName).toBe("PRE");
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  test("without an upstream the push sets one; on a detached HEAD there is no push", async () => {
    const { sent, open, submit } = await setup({ branch: "topic" });
    await open("推送");
    expect(screen.getByText("这个分支还没有上游，推送时会设置上游")).toBeTruthy();
    await submit();
    expect(sent()).toEqual([["git/push", expect.objectContaining({ set_upstream: true })]]);
    cleanup();

    const detached = await setup({});
    expect(screen.getByRole("button", { name: "Git 操作" })).toBeTruthy();
    await detached.open("提交…");
    expect(screen.getByText("当前是分离的 HEAD，只能提交，不能推送")).toBeTruthy();
    expect(screen.getByRole("radio", { name: "推送" })).toHaveProperty("disabled", true);
    expect(screen.getByRole("radio", { name: "提交并推送" })).toHaveProperty("disabled", true);
  });

  test("each failure code says what to do", async () => {
    let reply: Reply = hostError("git_error");
    const { open, submit } = await setup(BRANCH, (method) =>
      method === "git/commit" ? reply : undefined,
    );
    await open("提交…");
    const cases: [Reply, RegExp][] = [
      [hostError("git_error", "signing_failed"), /pinentry-mac/],
      [hostError("git_error", "nothing_to_commit"), /包含未暂存的更改/],
      [hostError("git_error", "lock_held"), /另一个 git 进程/],
      [hostError("conflict", "outcome_unknown"), /查看提交记录后再决定/],
      [hostError("git_error"), /操作没有完成（git_error）/],
    ];
    for (const [error, text] of cases) {
      reply = error;
      await submit("修登录");
      expect(screen.getByRole("alert").textContent).toMatch(text);
    }
  });

  test("the push's codes, the configuration notice, and Esc gives the focus back", async () => {
    let reply: Reply = hostError("git_error", "auth_failed");
    const changed = { ...BRANCH, repo_config_changed: [".git/hooks/pre-push"] };
    const { open, submit } = await setup(changed, (method) =>
      method === "git/push" ? reply : undefined,
    );
    await open("推送");
    expect(screen.getByText(/可执行配置有变化：\.git\/hooks\/pre-push/)).toBeTruthy();
    await submit();
    expect(screen.getByRole("alert").textContent).toMatch(/SSH_AUTH_SOCK/);
    reply = hostError("git_error", "rejected");
    await submit();
    expect(screen.getByRole("alert").textContent).toMatch(/先拉取远端的新提交/);
    // Esc, from inside the dialog: the key comes first, then the dialog's cancel.
    screen.getByRole("radio", { name: "推送" }).focus();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    fireEvent(screen.getByRole("dialog"), new Event("cancel", { cancelable: true }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Git 操作" }));
  });
});
