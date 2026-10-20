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
import { NewSessionPage } from "./NewSessionPage";

afterEach(cleanup);

const project = (project_id: string, name: string, position: number, trust_state: string) => ({
  type: "project.upserted",
  data: { project_id, name, position, trust_state, path: `/w/${name}`, is_git: true, added_at: 1 },
});
const PROJECTS = [project("p_1", "cupper", 0, "trusted"), project("p_2", "notes", 1, "untrusted")];
/** Claude without its CLI. */
const NO_CLI = {
  type: "agents.updated",
  data: {
    agents: [
      {
        agent: "claude",
        state: "unavailable",
        adapter: { runtime: "ready", version: "0.91.0" },
        auth: { state: "unchecked" },
        config_dir: "default",
      },
    ],
    git: {},
  },
};

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
  return { ui, controls, creates, user: userEvent.setup() };
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

  test("session/create takes the agent and the workspace picked", async () => {
    const { creates } = await setup(PROJECTS);
    const area = screen.getByRole("textbox");
    fireEvent.change(area, { target: { value: "开始吧" } });
    await act(async () => void fireEvent.keyDown(area, { key: "Enter" }));
    expect(creates()).toEqual([
      expect.objectContaining({ agent: "claude", workspace: { kind: "local" }, project_id: "p_1" }),
    ]);
    expect(screen.getByText("本地")).toBeTruthy();
  });

  test("without a project nothing is sent; an agent that cannot run says why", async () => {
    const { creates, user } = await setup([NO_CLI]);
    expect(screen.getByRole("heading").textContent).toBe("我们要构建什么？");
    const area = screen.getByRole("textbox");
    fireEvent.change(area, { target: { value: "开始吧" } });
    await act(async () => void fireEvent.keyDown(area, { key: "Enter" }));
    expect(creates()).toEqual([]);
    expect(document.querySelector("[data-send-state]")?.getAttribute("data-send-state")).toBe(
      "disabled",
    );
    await open(user, "选择 agent");
    expect(screen.getByRole("menuitem").textContent).toBe("Claude没有找到 CLI");
  });
});
