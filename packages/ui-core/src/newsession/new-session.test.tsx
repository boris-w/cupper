// @vitest-environment jsdom
import { HostClient } from "@cupper/host-client";
import { MockHost } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test } from "vitest";

import { HostContext, I18nProvider, ShellProvider } from "../app/providers";
import { RouterProvider } from "../app/router";
import { ToasterProvider } from "../feedback/toaster";
import { DEFAULT_PREFS } from "../layout/layout";
import { StateProvider, UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";
import { capabilities } from "../composer/test-capabilities";
import { NewSessionPage } from "./NewSessionPage";

afterEach(cleanup);

const project = (project_id: string, name: string, position: number, trust_state: string) => ({
  type: "project.upserted",
  data: { project_id, name, position, trust_state, path: `/w/${name}`, is_git: true, added_at: 1 },
});
const PROJECTS = [project("p_1", "cupper", 0, "trusted"), project("p_2", "notes", 1, "untrusted")];
const agent = (name: string, state: string, cli?: object) => ({
  ...{ agent: name, state, cli, config_dir: "default" },
  ...{ adapter: { runtime: "ready", version: "1" }, auth: { state: "unchecked" } },
});
const agents = (...list: object[]) => ({ type: "agents.updated", data: { agents: list, git: {} } });
/** Claude without its CLI; Codex ready. */
const NO_CLI = agents(
  agent("claude", "unavailable"),
  agent("codex", "ready", { path: "/bin/codex", install: "native", compat: "verified" }),
);
const value = (value: string, name: string, risk = "normal") => ({ value, name, risk });
/** Codex's cached capabilities: three permission modes. */
const CACHED = {
  captured_at: 1,
  capabilities: capabilities({
    agent: "codex",
    config: [
      {
        ...{ id: "permission_mode", agent_option_id: "mode", current: "read-only" },
        values: [
          value("read-only", "Read only"),
          value("auto", "Auto", "elevated"),
          value("full-access", "Full access", "full_access"),
        ],
      },
    ],
  }).data,
};
const BRANCHES = { current: "main", local: ["main", "dev"], remote: ["origin/main"] };
/** The host's settings, Codex's default permission `codex`. */
const settings = (codex: string | null) => ({
  type: "settings.updated",
  data: {
    settings: {
      ...{ "agents.claude.cli_path": null, "agents.codex.cli_path": null },
      ...{ "agents.env_passthrough": [], "worktrees.root": "/w", "diagnostics.verbose": false },
      ...{ "notifications.needs_action": true, "notifications.turn_completed": true },
      ...{ "notifications.error": true, "apps.terminal": "Terminal", "apps.editor": "TextEdit" },
      ...{ "sessions.default_agent": "claude", "sessions.default_permission.claude": null },
      ...{ "sessions.default_permission.codex": codex, "host.idle_exit_minutes": 10 },
      "host.prevent_idle_sleep": true,
    },
  },
});

/** The new session page on an index of `ops`. */
async function setup(ops: object[]) {
  const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
  mock.handle("stream/subscribe", ({ id, conn, params }) => {
    const { stream } = params as { stream: string };
    conn.reply(id, { result: { stream, mode: "snapshot", at_seq: 1, head_seq: 1, epoch: 1 } });
    // The new session, once created, has nothing yet.
    const page = {
      stream,
      at_seq: 1,
      epoch: 1,
      page: 0,
      last: true,
      ops: stream === "index" ? ops : [],
    };
    conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: page });
    return undefined;
  });
  mock.handle("session/create", () => ({ result: { session_id: "s_9" } }));
  // Claude's cache fills once `agents/refresh` probed it.
  let probed = false;
  mock.handle("agents/refresh", () => {
    probed = true;
    return { result: { status: {} } };
  });
  mock.handle("agents/capabilities", ({ params }) => ({
    result: (params as { agent: string }).agent === "codex" || probed ? { cached: CACHED } : {},
  }));
  mock.handle("git/branches", () => ({ result: BRANCHES }));
  const client = new HostClient();
  const conn = mock.connect();
  conn.initialized = true;
  client.attach(conn.port(), {} as Handshake);
  const { shell, controls } = createBrowserShell();
  const ui = createUiStore({ prefs: DEFAULT_PREFS, overlayOpen: false }, { page: "new" });
  render(
    <ShellProvider api={shell} info={await shell.init()}>
      <I18nProvider locale="zh-CN">
        <ToasterProvider>
          <HostContext value={{ client, state: client.state, handshake: undefined }}>
            <UiProvider ui={ui}>
              <StateProvider>
                <RouterProvider>
                  <NewSessionPage />
                </RouterProvider>
              </StateProvider>
            </UiProvider>
          </HostContext>
        </ToasterProvider>
      </I18nProvider>
    </ShellProvider>,
  );
  await act(() => new Promise((done) => requestAnimationFrame(() => done(undefined))));
  const creates = () =>
    mock.requests.filter((r) => r.method === "session/create").map((r) => r.params);
  return { ui, controls, creates, mock, user: userEvent.setup() };
}

const open = async (user: ReturnType<typeof userEvent.setup>, name: string) => {
  screen.getByRole("button", { name }).focus();
  await user.keyboard("{Enter}");
};

describe("UI-new-session", () => {
  test("the project's name picks the project; the list ends with adding one", async () => {
    const { ui, controls, user } = await setup(PROJECTS);
    expect(screen.getByRole("heading").textContent).toBe("你想在cupper中构建什么？");
    await open(user, "选择项目");
    expect(screen.getByRole("menuitem", { name: "notes未信任" })).toBeTruthy();
    await user.click(screen.getByRole("menuitem", { name: "notes未信任" }));
    expect(ui.getState().route).toEqual({ page: "new", projectId: "p_2" });
    expect(screen.getByRole("button", { name: "选择项目" }).textContent).toBe("notes");
    await open(user, "选择项目");
    await user.click(screen.getByRole("menuitem", { name: "添加项目" }));
    expect(controls.privilegedActions).toEqual([{ kind: "add_project" }]);
  });

  test("session/create takes the agent, the workspace, the base and the options picked", async () => {
    const { ui, creates, mock, user } = await setup([...PROJECTS, NO_CLI]);
    await open(user, "选择 agent");
    await user.click(screen.getByRole("menuitemradio", { name: "Codex" }));
    // Codex's cached modes, before the first message; the looser one says so.
    await open(user, "权限");
    await user.click(await screen.findByRole("menuitemradio", { name: /^Auto/ }));
    expect(screen.getByRole("status").textContent).toMatch(/^低打扰模式已开启/);
    await open(user, "选择工作位置");
    await user.click(screen.getByRole("menuitemradio", { name: "新建 worktree" }));
    await open(user, "选择起始分支");
    expect(screen.getByRole("menuitemradio", { checked: true }).textContent).toBe("main");
    await user.click(screen.getByRole("menuitemradio", { name: "origin/main" }));
    const area = screen.getByRole("textbox");
    fireEvent.change(area, { target: { value: "开始吧" } });
    await act(async () => void fireEvent.keyDown(area, { key: "Enter" }));
    expect(creates()).toEqual([
      expect.objectContaining({
        ...{ agent: "codex", project_id: "p_1", config: { permission_mode: "auto" } },
        workspace: { kind: "worktree", base_ref: "origin/main" },
      }),
    ]);
    expect(mock.requests.find((r) => r.method === "git/branches")?.params).toEqual({
      project_id: "p_1",
    });
    expect(ui.getState().chosen).toEqual({});
    expect(ui.getState().route).toEqual({ page: "session", sessionId: "s_9" });
  });

  test("an empty cache is probed once, then read again", async () => {
    const { mock } = await setup(PROJECTS);
    expect(await screen.findByRole("button", { name: "权限" })).toBeTruthy();
    const asked = mock.requests.map((r) => r.method).filter((m) => m.startsWith("agents/"));
    expect(asked).toEqual(["agents/capabilities", "agents/refresh", "agents/capabilities"]);
  });

  test("Q44: the default permission is preselected and sent; one the agent lost is not", async () => {
    const send = async (user: ReturnType<typeof userEvent.setup>) => {
      await open(user, "选择 agent");
      await user.click(screen.getByRole("menuitemradio", { name: "Codex" }));
      const mode = (await screen.findByRole("button", { name: "权限" })).textContent;
      const area = screen.getByRole("textbox");
      fireEvent.change(area, { target: { value: "开始吧" } });
      await act(async () => void fireEvent.keyDown(area, { key: "Enter" }));
      return mode;
    };
    const auto = await setup([...PROJECTS, NO_CLI, settings("auto")]);
    expect(await send(auto.user)).toBe("Auto");
    expect(auto.creates()).toEqual([
      expect.objectContaining({ agent: "codex", config: { permission_mode: "auto" } }),
    ]);
    cleanup();
    const gone = await setup([...PROJECTS, NO_CLI, settings("gone")]);
    expect(await send(gone.user)).toBe("Read only");
    expect(gone.creates()).toEqual([expect.objectContaining({ agent: "codex", config: {} })]);
    cleanup();
    // Full access by default still asks each time.
    const full = await setup([...PROJECTS, NO_CLI, settings("full-access")]);
    await send(full.user);
    expect(full.creates()).toEqual([]);
    expect(full.controls.privilegedActions).toEqual([
      expect.objectContaining({ kind: "create_session" }),
    ]);
  });

  test("full access is created through the native confirmation; cancelled, the draft stays", async () => {
    const { ui, creates, controls, user } = await setup([...PROJECTS, NO_CLI]);
    ui.getState().choose("codex", "permission_mode", "full-access");
    await open(user, "选择 agent");
    await user.click(screen.getByRole("menuitemradio", { name: "Codex" }));
    const area = screen.getByRole("textbox");
    fireEvent.change(area, { target: { value: "开始吧" } });
    await act(async () => void fireEvent.keyDown(area, { key: "Enter" }));
    expect(creates()).toEqual([]);
    expect(controls.privilegedActions).toEqual([
      {
        kind: "create_session",
        params: expect.objectContaining({
          ...{ agent: "codex", workspace: { kind: "local" }, project_id: "p_1" },
          config: { permission_mode: "full-access" },
          command_id: expect.any(String),
        }),
      },
    ]);
    expect(ui.getState().drafts.new).toBe("开始吧");
    controls.setPrivilegedResult({ status: "confirmed", result: { session_id: "s_7" } });
    await act(async () => void fireEvent.keyDown(area, { key: "Enter" }));
    expect(ui.getState().route).toEqual({ page: "session", sessionId: "s_7" });
  });

  test("an agent that cannot run says why and sends nothing; without a project neither", async () => {
    const { creates, user } = await setup([...PROJECTS, NO_CLI]);
    expect(screen.getByText("没有找到 CLI")).toBeTruthy();
    await open(user, "选择 agent");
    expect(screen.getAllByRole("menuitemradio").map((item) => item.textContent)).toEqual([
      "Claude没有找到 CLI",
      "Codex",
    ]);
    await user.keyboard("{Escape}");
    const area = screen.getByRole("textbox");
    fireEvent.change(area, { target: { value: "开始吧" } });
    await act(async () => void fireEvent.keyDown(area, { key: "Enter" }));
    expect(creates()).toEqual([]);
    expect(area.getAttribute("placeholder")).toBe("描述你要做的事");
    cleanup();

    await setup([NO_CLI]);
    expect(screen.getByRole("heading").textContent).toBe("我们要构建什么？");
    expect(screen.getByRole("textbox").getAttribute("placeholder")).toBe("先添加一个项目");
    expect(document.querySelector("[data-send-state]")?.getAttribute("data-send-state")).toBe(
      "disabled",
    );
  });

  test("a project that is no repository's top has no worktree", async () => {
    const { user } = await setup([
      { ...PROJECTS[0], data: { ...PROJECTS[0]?.data, is_git: false } },
    ]);
    await open(user, "选择工作位置");
    const worktree = screen.getByRole("menuitemradio", { name: /新建 worktree/ });
    expect(worktree.getAttribute("aria-disabled")).toBe("true");
    expect(worktree.textContent).toContain("项目不是 Git 仓库，或是仓库的子目录");
  });
});
