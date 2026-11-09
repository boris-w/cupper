// @vitest-environment jsdom
import { HostClient } from "@cupper/host-client";
import { MockHost, type Reply } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { HostContext, I18nProvider, ShellProvider } from "../app/providers";
import { RouterProvider, useRouter } from "../app/router";
import { ToasterProvider } from "../feedback/toaster";
import { DEFAULT_PREFS } from "../layout/layout";
import { SettingsRow } from "../sidebar/parts";
import { StateProvider, UiProvider } from "../state/provider";
import { createUiStore, toSettings } from "../state/ui";
import { SettingsPage } from "./SettingsPage";

afterEach(cleanup);

/** Claude running; Codex without its CLI, its runtime failed, with a value this build does not know. */
const AGENTS = {
  type: "agents.updated",
  data: {
    agents: [
      {
        ...{ agent: "claude", state: "running", rss_bytes: 734003200, config_dir: "ok" },
        cli: {
          path: "/opt/homebrew/bin/claude",
          version: "2.1.296",
          compat: "verified",
          install: "native",
        },
        adapter: { version: "0.91.0", runtime: "ready" },
        auth: { state: "logged_in", kind: "subscription", plan: "max" },
      },
      {
        ...{ agent: "codex", state: "unavailable", config_dir: "elsewhere" },
        adapter: { version: "2.2.2", runtime: "failed", failure: "network" },
        auth: { state: "logged_out" },
      },
    ],
    git: { version: "2.50.1" },
  },
};

/** A desktop window, and the CLI on weak authentication with a grant. */
const CLIENTS = {
  type: "clients.updated",
  data: {
    clients: [
      { conn_id: "c_1", principal_kind: "desktop_ui", role: "window", auth: "verified" },
      {
        conn_id: "c_2",
        principal_kind: "cli",
        auth: "weak",
        elevated: { scopes: [], expires_at: 9 },
      },
    ].map((c, n) => ({
      client_name: ["Cupper", "cupper-cli"][n],
      client_version: "0.1.0",
      since: 1,
      ...c,
    })),
  },
};
const PROJECT = {
  type: "project.upserted",
  data: {
    ...{ project_id: "p_1", name: "cupper", path: "/w/cupper", position: 0 },
    ...{ trust_state: "trusted", is_git: true, added_at: 1 },
  },
};
const archived = (n: number, title: string) => ({
  summary: { session_id: `s_${n}`, project_id: "p_1", title },
  archived_at: 1700049600000, // noon UTC: the same date in any time zone
});
/** Two pages: the first one's cursor leads to the second. */
const PAGES = [
  { sessions: [archived(8, "旧的会话")], next: { archived_at: 1700049600000, session_id: "s_8" } },
  { sessions: [archived(9, "")] },
];

async function setup() {
  const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
  mock.handle("stream/subscribe", ({ id, conn, params }) => {
    const { stream } = params as { stream: string };
    conn.reply(id, { result: { stream, mode: "snapshot", at_seq: 1, head_seq: 1, epoch: 1 } });
    const ops = stream === "index" ? [AGENTS, CLIENTS, PROJECT] : [];
    const page = { stream, at_seq: 1, epoch: 1, page: 0, last: true, ops };
    conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: page });
    return undefined;
  });
  // The host answers restarts and restores when the test says.
  const held: ((answer: Reply) => void)[] = [];
  for (const method of ["agents/restart", "session/restore"]) {
    mock.handle(method, ({ id, conn }) => void held.push((answer) => conn.reply(id, answer)));
  }
  mock.handle("session/list_archived", ({ params }) => ({
    result: PAGES[(params as { cursor?: object }).cursor ? 1 : 0],
  }));
  const client = new HostClient();
  const conn = mock.connect();
  conn.initialized = true;
  client.attach(conn.port(), {} as Handshake);
  const { shell, controls } = createBrowserShell();
  const ui = createUiStore(
    { prefs: DEFAULT_PREFS, overlayOpen: false },
    { page: "session", sessionId: "s_1" },
  );
  const Main = () => {
    const { route } = useRouter();
    return route.page === "settings" && <SettingsPage route={route} />;
  };
  render(
    <ShellProvider api={shell} info={await shell.init()}>
      <I18nProvider locale="zh-CN">
        <ToasterProvider>
          <HostContext value={{ client, state: client.state, handshake: undefined }}>
            <UiProvider ui={ui}>
              <StateProvider>
                <RouterProvider>
                  <SettingsRow />
                  <Main />
                </RouterProvider>
              </StateProvider>
            </UiProvider>
          </HostContext>
        </ToasterProvider>
      </I18nProvider>
    </ShellProvider>,
  );
  await act(() => new Promise((done) => requestAnimationFrame(() => done(undefined))));
  return { controls, mock, held, ui };
}

const click = (name: string, within_: HTMLElement = document.body) =>
  fireEvent.click(within(within_).getByRole("button", { name }));
const card = (name: string) => screen.getByRole("region", { name });
const texts = (region: HTMLElement) =>
  [...region.querySelectorAll(".px-4 > *")].map((part) => part.textContent);

describe("UI-settings-agents", () => {
  test("the menu, the sidebar's row and 返回应用: back to where the settings came from", async () => {
    const { controls, ui } = await setup();
    const from = { page: "session", sessionId: "s_1" };
    act(() => controls.command({ id: "settings" }));
    expect(ui.getState().route).toEqual({ page: "settings", from });
    const nav = screen.getByRole("button", { name: "已归档会话" });
    expect(nav.getAttribute("aria-current")).toBe("page");
    act(() => controls.navigate({ kind: "settings", section: "agents" }));
    expect(ui.getState().route).toEqual({ page: "settings", section: "agents", from });
    click("返回应用");
    expect(ui.getState().route).toEqual(from);
    click("设置");
    expect(ui.getState().route).toEqual({ page: "settings", from });
  });

  test("each agent's card; a value this build does not know reads 未知", async () => {
    const { ui } = await setup();
    act(() => ui.getState().navigate(toSettings(ui.getState().route, "agents", "codex")));
    expect(texts(card("Claude"))).toEqual([
      "状态运行中",
      "命令行工具/opt/homebrew/bin/claude",
      "2.1.296 · 已验证",
      "适配器运行时就绪",
      "0.91.0",
      "登录已登录",
      "subscription · max",
      "配置目录使用登录 shell 里设置的目录",
      "内存占用",
      "700 MB",
    ]);
    expect(texts(card("Codex"))).toEqual([
      "状态不可用：没有找到 CLI",
      "命令行工具没有找到。在终端里安装后，点重启重新查找",
      "适配器运行时安装失败：网络错误",
      "2.2.2",
      "登录未登录",
      "在终端里运行以登录",
      "codex login",
      "配置目录未知",
    ]);
    expect(texts(card("Git"))).toEqual(["git", "2.50.1", "git-lfs", "没有找到"]);
    // An error's recovery step brings its agent's card into view.
    expect(document.activeElement).toBe(card("Codex"));
  });

  test("restart: agents/restart for the agent, busy until the host answers; a failure says so", async () => {
    const { mock, held, ui } = await setup();
    act(() => ui.getState().navigate(toSettings(ui.getState().route, "agents")));
    click("重启", card("Codex"));
    expect(mock.requests.at(-1)?.params).toEqual({
      agent: "codex",
      command_id: expect.any(String),
    });
    const button = () => within(card("Codex")).getByRole<HTMLButtonElement>("button");
    expect([button().textContent, button().disabled]).toEqual(["正在重启", true]);
    const error = { code: -32000, message: "down", data: { code: "unavailable" } };
    await act(async () => held[0]?.({ error }));
    expect(screen.getByRole("status").textContent).toBe("重启失败（unavailable）");
    expect([button().textContent, button().disabled]).toEqual(["重启", false]);
  });
});

describe("UI-settings-archived", () => {
  test("a page at a time; a restore is busy until the host answers, then says how it came back", async () => {
    const { mock, held, ui } = await setup();
    act(() => ui.getState().navigate(toSettings(ui.getState().route)));
    await act(() => new Promise((done) => setTimeout(done)));
    click("加载更多");
    await act(() => new Promise((done) => setTimeout(done)));
    expect(texts(card("会话"))).toEqual([
      "旧的会话cupper · 归档于 2023/11/15",
      "恢复",
      "未命名会话cupper · 归档于 2023/11/15",
      "恢复",
    ]);
    const lists = mock.requests.filter((request) => request.method === "session/list_archived");
    expect(lists.map((request) => request.params)).toEqual([{}, { cursor: PAGES[0]?.next }]);
    fireEvent.click(
      within(card("会话")).getAllByRole("button", { name: "恢复" })[0] as HTMLElement,
    );
    expect(mock.requests.at(-1)?.params).toEqual({
      session_id: "s_8",
      command_id: expect.any(String),
    });
    expect(
      within(card("会话")).getByRole<HTMLButtonElement>("button", { name: "正在恢复" }).disabled,
    ).toBe(true);
    await act(async () => held[0]?.({ result: { mode: "read_only" } }));
    expect(screen.getByRole("status").textContent).toBe(
      "已恢复「旧的会话」，但 worktree 的分支已经不在了，只能查看",
    );
  });

  test("connections: the local clients, what weak authentication means, no devices yet", async () => {
    const { ui } = await setup();
    act(() => ui.getState().navigate(toSettings(ui.getState().route, "connections")));
    expect(texts(card("本机客户端"))).toEqual([
      "Cupper桌面端 · 窗口 · 已校验代码签名",
      "0.1.0",
      "cupper-cli命令行工具 · 弱认证 · 已临时提权",
      "0.1.0",
      "弱认证开发构建的降级模式：连接凭启动时的一次性密钥认证，没有校验代码签名",
    ]);
    expect(texts(card("设备"))).toEqual(["还没有设备以后的版本支持从手机连接"]);
  });
});
