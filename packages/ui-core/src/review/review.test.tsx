// @vitest-environment jsdom
import { HostClient } from "@cupper/host-client";
import { hostError, MockHost, type Reply } from "@cupper/host-client/mock";
import type { GitDiffResult } from "@cupper/protocol";
import type { Handshake } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { HostContext, I18nProvider, ShellProvider } from "../app/providers";
import { SlotsContext } from "../app/slots";
import { TooltipProvider } from "../components/Tooltip";
import { ToasterProvider } from "../feedback/toaster";
import { DEFAULT_PREFS } from "../layout/layout";
import { StateProvider, UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";
import { ReviewPanel } from "./ReviewPanel";
import type { ReviewViewProps } from "./view";

beforeEach(() => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] }));
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const file = (path: string) =>
  ({ path, additions: 2, deletions: 1, binary: false, change: "update" }) as const;
const diff = (token: string, paths: string[], caveats: string[] = []): GitDiffResult => ({
  caveats,
  diff_token: token,
  files: paths.map(file),
  stat: { added: 2 * paths.length, removed: paths.length, files: paths.length },
});
const DIFFS: Readonly<Record<string, GitDiffResult>> = {
  unstaged: diff("dt_u", ["src/a.rs", "src/b.rs"]),
  staged: diff("dt_s", ["src/a.rs"]),
  last_turn: diff("dt_t", ["src/a.rs", "src/c.rs"], ["mixed_changes"]),
};

const COMMENT = {
  ...{ path: "src/a.rs", line_start: 2, line_end: 2, side: "new", quote: "let a = 1;" },
  comment: "改名",
} as const;

/** A diff view standing in for T131's: buttons that ask what the review document may ask. */
function View(props: ReviewViewProps) {
  return (
    <div>
      <p>{`${props.diff.diff_token} ${props.drafts.length} 条草稿`}</p>
      <button onClick={() => props.act("revert", { paths: ["src/a.rs"] }, "src/a.rs")}>
        还原 a
      </button>
      <button onClick={() => props.act("revert", { paths: ["src/b.rs"] }, "src/b.rs")}>
        还原 b
      </button>
      <button onClick={() => props.act("stage", { hunk_ids: ["hk_1"] })}>暂存 hunk</button>
      <button onClick={() => props.comment(COMMENT)}>评论</button>
    </div>
  );
}

async function setup(
  opts: {
    reply?: (method: string) => Reply | undefined;
    status?: object;
    diffs?: Record<string, GitDiffResult>;
  } = {},
) {
  const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
  mock.handle("stream/subscribe", ({ params }) => {
    const { stream } = params as { stream: string };
    return { result: { stream, mode: "snapshot", at_seq: 0, head_seq: 0, epoch: 1 } };
  });
  mock.handle("git/diff", ({ params }) => {
    const { scope } = params as { scope: string };
    return opts.reply?.("git/diff") ?? { result: { ...DIFFS, ...opts.diffs }[scope] };
  });
  mock.handle("git/status", () => ({
    result: { ahead: 0, behind: 0, files: [], ...opts.status },
  }));
  for (const method of ["git/stage", "git/unstage", "git/revert"]) {
    mock.handle(method, () => opts.reply?.(method) ?? { result: {} });
  }
  const client = new HostClient();
  const conn = mock.connect();
  conn.initialized = true;
  client.attach(conn.port(), {} as Handshake);
  const { shell } = createBrowserShell();
  const info = await shell.init();
  const ui = createUiStore(
    { prefs: DEFAULT_PREFS, overlayOpen: false },
    { page: "session", sessionId: "s_1" },
  );
  render(
    <ShellProvider api={shell} info={info}>
      <I18nProvider locale="zh-CN">
        <ToasterProvider>
          <HostContext value={{ client, state: client.state, handshake: undefined }}>
            <UiProvider ui={ui}>
              <StateProvider>
                <TooltipProvider>
                  <SlotsContext value={{ reviewView: View }}>
                    <ReviewPanel fileTree={false} onFileTree={() => undefined} active />
                  </SlotsContext>
                </TooltipProvider>
              </StateProvider>
            </UiProvider>
          </HostContext>
        </ToasterProvider>
      </I18nProvider>
    </ShellProvider>,
  );
  const settle = () => act(() => vi.advanceTimersByTimeAsync(0));
  await settle();
  const calls = (method: string) =>
    mock.requests
      .filter((r) => r.method === method)
      .map((r) => r.params as Record<string, unknown>);
  const scope = async (name: string) => {
    fireEvent.keyDown(screen.getByRole("button", { name: "查阅来源" }), { key: "Enter" });
    await settle();
    fireEvent.click(screen.getByRole("menuitemradio", { name }));
    await settle();
  };
  const click = async (name: string) => {
    fireEvent.click(screen.getByRole("button", { name }));
    await settle();
  };
  return { ui, calls, scope, click, settle };
}

describe("UNIT-review-actions", () => {
  test("the pill follows the scope, and each action names the token of its scope", async () => {
    const { calls, scope, click } = await setup();
    expect(screen.getByText("dt_u 0 条草稿")).toBeTruthy();
    expect(screen.getByText("+4")).toBeTruthy();
    await click("暂存全部");
    expect(calls("git/stage")).toEqual([
      expect.objectContaining({ session_id: "s_1", diff_token: "dt_u", target: { all: true } }),
    ]);
    expect(screen.getByText("已暂存")).toBeTruthy();
    // The panel loads again after any action.
    expect(calls("git/diff").length).toBeGreaterThan(1);

    await scope("已暂存");
    expect(screen.queryByRole("button", { name: "还原全部" })).toBeNull();
    // What a scope does not offer, the view cannot ask for either.
    await click("暂存 hunk");
    await click("还原 a");
    expect(calls("git/stage")).toHaveLength(1);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    await click("取消暂存全部");
    expect(calls("git/unstage")).toEqual([
      expect.objectContaining({ diff_token: "dt_s", target: { all: true } }),
    ]);

    // A turn's changes stage by their paths on the unstaged diff, the one the host stages from.
    await scope("最近一轮");
    expect(screen.getByText(/混有你自己或其他会话的改动/)).toBeTruthy();
    await click("暂存全部");
    expect(calls("git/stage").at(-1)).toEqual(
      expect.objectContaining({ diff_token: "dt_u", target: { paths: ["src/a.rs"] } }),
    );
  });

  test("revert all asks first: focus on 确认, armed after 500 ms, no revert on 取消", async () => {
    const { calls, click, settle } = await setup();
    await click("还原全部");
    const confirm = screen.getByRole("button", { name: "确认" });
    expect(screen.getByRole("alertdialog", { name: "是否还原更改？" })).toBeTruthy();
    expect(document.activeElement).toBe(confirm);
    // Keys and clicks before the control is armed do nothing; a button pressed before stays so.
    fireEvent.click(confirm);
    fireEvent.pointerDown(confirm);
    await act(() => vi.advanceTimersByTimeAsync(500));
    fireEvent.click(confirm, { detail: 1 });
    expect(calls("git/revert")).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    await settle();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(calls("git/revert")).toEqual([]);

    await click("还原全部");
    await act(() => vi.advanceTimersByTimeAsync(500));
    fireEvent.click(screen.getByRole("button", { name: "确认" }));
    await settle();
    expect(calls("git/revert")).toEqual([
      expect.objectContaining({ diff_token: "dt_u", target: { all: true } }),
    ]);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  test("the diff view's revert takes no focus, one at a time; stage and comments go at once", async () => {
    const { ui, calls, click } = await setup();
    await click("还原 a");
    const dialog = screen.getByRole("alertdialog", { name: "是否还原 src/a.rs？" });
    expect(dialog.contains(document.activeElement)).toBe(false);
    await click("还原 b");
    expect(screen.getAllByRole("alertdialog")).toHaveLength(1);
    expect(screen.getByRole("alertdialog", { name: "是否还原 src/a.rs？" })).toBeTruthy();
    await act(() => vi.advanceTimersByTimeAsync(500));
    await click("确认");
    expect(calls("git/revert")).toEqual([
      expect.objectContaining({ target: { paths: ["src/a.rs"] } }),
    ]);
    expect(screen.getByText("已还原 src/a.rs")).toBeTruthy();

    await click("暂存 hunk");
    expect(calls("git/stage")).toEqual([
      expect.objectContaining({ diff_token: "dt_u", target: { hunk_ids: ["hk_1"] } }),
    ]);
    await click("评论");
    expect(ui.getState().attachments.s_1).toEqual([{ type: "review_comment", ...COMMENT }]);
    expect(screen.getByText("dt_u 1 条草稿")).toBeTruthy();
  });

  test("a stale token says so and loads again; refusals show their code", async () => {
    let reply: Reply | undefined = hostError("conflict", "stale_diff");
    const { calls, click } = await setup({ reply: (m) => (m === "git/stage" ? reply : undefined) });
    const before = calls("git/diff").length;
    await click("暂存全部");
    expect(screen.getByText("变更已经更新，请再操作一次")).toBeTruthy();
    expect(calls("git/diff").length).toBeGreaterThan(before);
    reply = hostError("git_error", "lock_held");
    await click("暂存全部");
    expect(screen.getByText("操作没有完成（lock_held）")).toBeTruthy();
  });

  test("notices, empty states and a folder that is no repository", async () => {
    const { scope } = await setup({
      status: { repo_config_changed: [".git/hooks/pre-commit"] },
      diffs: { staged: diff("dt_s", []) },
    });
    expect(screen.getByText(/仓库的可执行配置有变化：\.git\/hooks\/pre-commit/)).toBeTruthy();
    await scope("已暂存");
    expect(screen.getByText("没有已暂存的更改")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "取消暂存全部" })).toBeNull();
    cleanup();
    await setup({
      reply: (m) => (m === "git/diff" ? hostError("git_error", "not_a_repo") : undefined),
    });
    expect(screen.getByText("这不是 Git 仓库")).toBeTruthy();
  });
});
