// @vitest-environment jsdom
import { HostClient } from "@cupper/host-client";
import { MockHost } from "@cupper/host-client/mock";
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

async function setup() {
  const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
  mock.handle("stream/subscribe", ({ id, conn, params }) => {
    const { stream } = params as { stream: string };
    conn.reply(id, { result: { stream, mode: "snapshot", at_seq: 1, head_seq: 1, epoch: 1 } });
    const ops = stream === "index" ? [AGENTS] : [];
    const page = { stream, at_seq: 1, epoch: 1, page: 0, last: true, ops };
    conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: page });
    return undefined;
  });
  const restarts: (() => void)[] = [];
  mock.handle("agents/restart", ({ id, conn }) => {
    const error = { code: -32000, message: "down", data: { code: "unavailable" } };
    restarts.push(() => conn.reply(id, { error }));
    return undefined;
  });
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
  return { controls, mock, restarts, ui };
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
    const nav = screen.getByRole("button", { name: "agent 状态" });
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
    const { mock, restarts, ui } = await setup();
    act(() => ui.getState().navigate(toSettings(ui.getState().route)));
    click("重启", card("Codex"));
    expect(mock.requests.at(-1)?.params).toEqual({
      agent: "codex",
      command_id: expect.any(String),
    });
    const button = () => within(card("Codex")).getByRole<HTMLButtonElement>("button");
    expect([button().textContent, button().disabled]).toEqual(["正在重启", true]);
    await act(async () => restarts[0]?.());
    expect(screen.getByRole("status").textContent).toBe("重启失败（unavailable）");
    expect([button().textContent, button().disabled]).toEqual(["重启", false]);
  });
});
