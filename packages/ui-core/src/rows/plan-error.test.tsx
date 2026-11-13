// @vitest-environment jsdom
import { HostClient } from "@cupper/host-client";
import { MockHost } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, test, vi } from "vitest";

import { HostContext, I18nProvider, ShellProvider } from "../app/providers";
import { ToasterProvider } from "../feedback/toaster";
import { DEFAULT_PREFS } from "../layout/layout";
import { StateProvider, UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";
import { runningPlan } from "../stream/ComposerOverlays";
import { StreamList } from "../stream/StreamList";
import { installResizeObserver } from "../stream/test-resize";
import type { StreamRow } from "./stream";

afterEach(cleanup);

// jsdom lays nothing out: the list is 800 px high, a row 40.
beforeAll(() => {
  installResizeObserver();
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return this.dataset.streamList === undefined ? 40 : 800;
    },
  });
});

const turn = (n: number, content: object[], completed?: string) => [
  { type: "turn.started", data: { turn_id: `t_${n}`, turn_no: n, started_at: n, origin: "user" } },
  item(`u_${n}`, "user_message", { content, steered: false }, n),
  ...(completed
    ? [
        {
          type: "turn.completed",
          data: { turn_id: `t_${n}`, status: completed, ended_at: n, final_item_ids: [] },
        },
      ]
    : []),
];
function item(item_id: string, kind: string, fields: object, n = 1) {
  return {
    type: "item.started",
    data: { item_id, turn_id: `t_${n}`, started_at: n, kind, ...fields },
  };
}
const error = (id: string, guidance: string, n: number) =>
  item(id, "error", { code: "other", guidance, retryable: false }, n);

/** Session s_1 of `agent`, with `ops`; its host answers `session/send`. */
async function setup(ops: object[], agent = "claude") {
  const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
  const summary = {
    type: "session.upserted",
    data: {
      ...{ session_id: "s_1", title: "t", status: "idle", agent, project_id: "p_1" },
      ...{ title_source: "prompt", last_activity_at: 1, pending_interactions: 0 },
      ...{ read_only: false, unread: false, workspace_kind: "local" },
    },
  };
  mock.handle("stream/subscribe", ({ id, conn, params }) => {
    const { stream } = params as { stream: string };
    conn.reply(id, { result: { stream, mode: "snapshot", at_seq: 1, head_seq: 1, epoch: 1 } });
    const page = { stream, at_seq: 1, epoch: 1, page: 0, last: true };
    const sent = { ...page, ops: stream === "index" ? [summary] : ops };
    conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: sent });
    return undefined;
  });
  mock.handle("session/send", () => ({ result: { disposition: "started" } }));
  const client = new HostClient();
  const conn = mock.connect();
  conn.initialized = true;
  client.attach(conn.port(), {} as Handshake);
  const { shell } = createBrowserShell();
  const ui = createUiStore(
    { prefs: DEFAULT_PREFS, overlayOpen: false },
    { page: "session", sessionId: "s_1" },
  );
  render(
    <ShellProvider api={shell} info={await shell.init()}>
      <I18nProvider locale="zh-CN">
        <ToasterProvider>
          <HostContext value={{ client, state: client.state, handshake: undefined }}>
            <UiProvider ui={ui}>
              <StateProvider>
                <StreamList sessionId="s_1" />
              </StateProvider>
            </UiProvider>
          </HostContext>
        </ToasterProvider>
      </I18nProvider>
    </ShellProvider>,
  );
  await act(() => new Promise((done) => requestAnimationFrame(() => done(undefined))));
  const requests = (method: string) =>
    mock.requests.filter((request) => request.method === method).map((request) => request.params);
  return Object.assign(requests, { ui });
}

const steps = (...statuses: string[]) => ({
  source: "entries",
  entries: statuses.map((status, n) => ({ content: `第 ${n + 1} 项‮`, status })),
});

describe("UI-plan", () => {
  test("steps are numbered with their status; one this build does not know is not started", async () => {
    await setup([
      ...turn(1, []),
      item("p_1", "plan", steps("completed", "in_progress", "x", "pending")),
    ]);
    const items = [...document.querySelectorAll("li[data-status]")];
    expect(items.map((li) => li.getAttribute("data-status"))).toEqual([
      "completed",
      "in_progress",
      "pending",
      "pending",
    ]);
    expect(screen.getAllByRole("img").map((icon) => icon.getAttribute("aria-label"))).toEqual([
      "已完成",
      "进行中",
      "未开始",
      "未开始",
    ]);
    expect(items[1]?.textContent).toBe("2.第 2 项\\u{202E}");
    // The turn runs: the pill names the step in progress.
    expect(screen.getByRole("button", { name: "第 2 / 4 步" })).toBeTruthy();
  });

  test("a Markdown plan is a bar that opens on it, 编写计划 while it grows", async () => {
    const markdown = { source: "markdown", markdown: { type: "text", text: "1. **先补测试**" } };
    await setup([...turn(1, []), item("p_1", "plan", markdown)]);
    const bar = screen.getByRole("button", { name: "编写计划" });
    expect(bar.querySelector(".shimmer")).toBeTruthy();
    fireEvent.click(bar);
    expect(screen.getByText("先补测试").tagName).toBe("STRONG");
    expect(screen.queryByRole("button", { name: /^第 / })).toBeNull();
  });

  test("the pill: the step in progress, else the first not done; none once the turn ends", () => {
    const row = (statuses: string[], status = "running"): StreamRow[] => [
      { kind: "turn_header", key: "t", turnId: "t", turnNo: 1, status, startedAt: 0 },
      { kind: "plan", key: "p", itemId: "p", plan: steps(...statuses), running: true },
    ];
    expect(runningPlan(row(["completed", "in_progress", "pending"]))).toEqual({
      key: "p",
      n: 2,
      m: 3,
    });
    expect(runningPlan(row(["completed", "pending"]))?.n).toBe(2);
    expect(runningPlan(row(["completed", "completed"]))?.n).toBe(2);
    expect(runningPlan(row(["pending"], "completed"))).toBeUndefined();
    expect(runningPlan(row([]))).toBeUndefined();
  });
});

describe("UI-error", () => {
  const KNOWN = [
    "run_login_in_terminal",
    "upgrade_cli",
    "install_cli",
    "quota_exhausted",
    "change_model",
    "restart_agent",
  ];
  const OPS = [
    ...turn(1, [{ type: "text", text: "第一次" }], "failed"),
    error("e_0", "retry", 1),
    ...turn(2, [
      { type: "text", text: "再来一次" },
      { type: "file_ref", path: "a.rs" },
    ]),
    ...[...KNOWN, "none", "page_admin"].map((guidance, n) => error(`e_${n + 1}`, guidance, 2)),
    error("e_9", "retry", 2),
  ];

  test("each step says what to do; none, and a step this build does not know, add nothing", async () => {
    const { ui } = await setup(OPS, "codex");
    const shown = [...document.querySelectorAll("[data-guidance]")];
    expect(shown.map((box) => box.getAttribute("data-guidance"))).toEqual(["retry", ...KNOWN]);
    expect(shown.map((box) => box.firstChild?.textContent)).toEqual([
      "重新发送消息即可重试。",
      "在终端里登录后重试。",
      "在终端里升级命令行工具后重试。",
      "在终端里安装命令行工具后重试。",
      "等额度恢复后再试，或者换一个账号。",
      "换一个模型后重试。",
      "重启 agent 后重试。",
    ]);
    expect(document.querySelectorAll("[role=alert]")).toHaveLength(10);
    // `change_model` opens the composer's model menu (T231).
    expect(screen.getAllByRole("button", { name: "换模型" })).toHaveLength(1);
    // The CLI's and the agent's steps open the agent's card on the settings page (T151).
    const status = screen.getAllByRole("button", { name: "查看 agent 状态" });
    expect(status).toHaveLength(3);
    fireEvent.click(status[2] as HTMLElement);
    expect(ui.getState().route).toEqual({
      ...{ page: "settings", section: "agents", focus: "codex" },
      from: { page: "session", sessionId: "s_1" },
    });
  });

  test("login: the agent's command, to copy into a terminal", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    await setup(OPS, "codex");
    expect(screen.getByText("codex login").tagName).toBe("CODE");
    fireEvent.click(screen.getByRole("button", { name: "复制命令" }));
    expect(writeText).toHaveBeenCalledWith("codex login");
  });

  test("retry, in the last turn only, sends the message that started it again as it was", async () => {
    const requests = await setup(OPS);
    const [retry, ...more] = screen.getAllByRole("button", { name: "重试" });
    expect(more).toEqual([]);
    await act(async () => void fireEvent.click(retry as HTMLElement));
    expect(requests("session/send")).toEqual([
      expect.objectContaining({
        session_id: "s_1",
        mode: "queue",
        content: [
          { type: "text", text: "再来一次" },
          { type: "file_ref", path: "a.rs" },
        ],
      }),
    ]);
  });
});
