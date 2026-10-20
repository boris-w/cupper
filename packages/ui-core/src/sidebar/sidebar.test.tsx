// @vitest-environment jsdom
import type { ProjectUpserted, SessionSummary } from "@cupper/protocol";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test } from "vitest";

import { I18nProvider, ShellProvider } from "../app/providers";
import { RouterProvider } from "../app/router";
import { TooltipProvider } from "../components/Tooltip";
import { ConnectionToast } from "../feedback/ConnectionToast";
import { feedbackOf, usePrivileged } from "../feedback/privileged";
import { MAX_TOASTS, ToasterProvider, useToasts, type Toasts } from "../feedback/toaster";
import { DEFAULT_PREFS } from "../layout/layout";
import { UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";
import { ElevationBanner, ProjectList } from "./parts";

afterEach(cleanup);

async function setup(node: ReactNode = null) {
  const { shell, controls } = createBrowserShell();
  const info = await shell.init();
  const ui = createUiStore({ prefs: DEFAULT_PREFS, overlayOpen: false });
  const handles: { toasts?: Toasts; run?: ReturnType<typeof usePrivileged> } = {};
  const Probe = () => {
    handles.toasts = useToasts();
    handles.run = usePrivileged();
    return null;
  };
  const wrap = (child: ReactNode) => (
    <ShellProvider api={shell} info={info}>
      <I18nProvider locale="zh-CN">
        <ToasterProvider>
          <UiProvider ui={ui}>
            <RouterProvider>
              <TooltipProvider>
                <Probe />
                {child}
              </TooltipProvider>
            </RouterProvider>
          </UiProvider>
        </ToasterProvider>
      </I18nProvider>
    </ShellProvider>
  );
  const { rerender } = render(wrap(node));
  return { controls, handles, ui, rerender: (child: ReactNode) => rerender(wrap(child)) };
}

const project = (project_id: string, name: string, trust_state = "trusted") =>
  ({ project_id, name, trust_state, position: 0 }) as ProjectUpserted;
const session = (n: number, status = "idle", extra: Partial<SessionSummary> = {}) =>
  ({
    ...{ session_id: `s_${n}`, title: `会话 ${n}`, status, agent: "claude", project_id: "p_1" },
    ...{ last_activity_at: 100 - n, ...extra },
  }) as SessionSummary;
const click = (name: string | RegExp) => fireEvent.click(screen.getByRole("button", { name }));

const toasts = () => screen.queryAllByRole("status").map((toast) => toast.textContent);

describe("UNIT-ui-sidebar", () => {
  test("each outcome of a privileged action gets its feedback", () => {
    expect(feedbackOf({ status: "confirmed" })).toBeUndefined();
    expect(feedbackOf({ status: "cancelled" })).toBeUndefined();
    expect(feedbackOf({ status: "busy" })?.key).toBe("shell:privileged_busy");
    expect(feedbackOf({ status: "cooldown", retry_after_ms: 1500 })).toEqual({
      key: "shell:privileged_cooldown",
      values: { seconds: 2 },
    });
    expect(feedbackOf({ status: "user_activation_required" })?.key).toBe(
      "shell:privileged_activation",
    );
    expect(feedbackOf({ status: "failed", error: { code: "forbidden" } })?.values).toEqual({
      code: "forbidden",
    });
    expect(feedbackOf({ status: "failed", error: "boom" })?.values).toEqual({ code: "unexpected" });
  });

  test("a privileged action that did not go through says so in a toast", async () => {
    const { controls, handles } = await setup();
    controls.setPrivilegedResult({ status: "cooldown", retry_after_ms: 10_000 });
    await act(() => handles.run?.({ kind: "add_project" }));
    expect(controls.privilegedActions).toEqual([{ kind: "add_project" }]);
    expect(toasts()).toEqual(["取消得太频繁了，请 10 秒后再试"]);
    controls.setPrivilegedResult({ status: "cancelled" });
    await act(() => handles.run?.({ kind: "add_project" }));
    expect(toasts()).toHaveLength(1);
  });

  test("the Toaster keeps the newest three and replaces a toast by id", async () => {
    const { handles } = await setup();
    act(() => {
      for (const id of ["a", "b", "c", "d"]) {
        handles.toasts?.show({ id, text: id });
      }
      handles.toasts?.show({ id: "c", text: "c2" });
    });
    expect(toasts()).toHaveLength(MAX_TOASTS);
    expect(toasts()).toEqual(["b", "d", "c2"]);
  });

  test("a lost connection stays in a toast until it is ready again", async () => {
    const { rerender } = await setup(<ConnectionToast state={{ status: "reconnecting" }} />);
    expect(toasts()).toEqual(["正在重新连接会话宿主"]);
    rerender(<ConnectionToast state={{ status: "failed", reason: "host_unreachable" }} />);
    expect(toasts()).toEqual(["无法连接会话宿主"]);
    rerender(<ConnectionToast state={{ status: "ready" }} />);
    expect(toasts()).toEqual([]);
  });

  test("session rows show their state and pending kind; a project not trusted says so", async () => {
    const sessions = [
      session(1, "running"),
      session(2, "needs_action", { pending_kind: "approval" }),
      session(3, "needs_action", { pending_kind: "input" }),
      session(4, "error", { error_code: "process_exited" }),
      session(5, "unread"),
    ];
    const projects = [project("p_1", "cupper"), project("p_2", "notes", "untrusted")];
    await setup(<ProjectList projects={projects} sessions={sessions} />);
    for (const name of ["运行中", "agent 进程已退出", "未读"]) {
      expect(screen.getByRole("img", { name })).toBeTruthy();
    }
    for (const text of ["等待批准", "需要用户输入", "未信任"]) {
      expect(screen.getAllByText(text)).toHaveLength(1);
    }
  });

  test("a project shows five sessions until expanded; rows open a session or a new one", async () => {
    const sessions = [1, 2, 3, 4, 5, 6].map((n) => session(n));
    const { ui } = await setup(
      <ProjectList projects={[project("p_1", "cupper")]} sessions={sessions} />,
    );
    expect(screen.queryByText("会话 6")).toBeNull();
    click("展开显示");
    click("会话 6");
    expect(ui.getState().route).toEqual({ page: "session", sessionId: "s_6" });
    expect(screen.getByRole("button", { name: "会话 6" }).getAttribute("aria-current")).toBe(
      "page",
    );
    click("收起");
    expect(screen.queryByText("会话 6")).toBeNull();
    click("在 cupper 中新建会话");
    expect(ui.getState().route).toEqual({ page: "new", projectId: "p_1" });
  });

  test("adding a project and reviewing the oldest elevation go through requestPrivileged", async () => {
    const elevation = (elevation_id: string, requested_at: number) => ({
      ...{ elevation_id, requested_at, client_name: "cupper-cli", scopes: ["privileged"] },
    });
    const { controls } = await setup(
      <>
        <ProjectList projects={[]} sessions={[]} />
        <ElevationBanner elevations={[elevation("e_2", 2), elevation("e_1", 1)]} />
      </>,
    );
    expect(screen.getByText("另有 1 条申请")).toBeTruthy();
    click("添加项目");
    click("查看");
    await act(() => Promise.resolve());
    expect(controls.privilegedActions).toEqual([
      { kind: "add_project" },
      { kind: "elevation", elevation_id: "e_1" },
    ]);
  });
});
