// @vitest-environment jsdom
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test } from "vitest";

import { I18nProvider, ShellProvider } from "../app/providers";
import { ConnectionToast } from "./ConnectionToast";
import { feedbackOf, usePrivileged } from "./privileged";
import { MAX_TOASTS, ToasterProvider, useToasts, type Toasts } from "./toaster";

afterEach(cleanup);

async function setup(node: ReactNode = null) {
  const { shell, controls } = createBrowserShell();
  const info = await shell.init();
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
          <Probe />
          {child}
        </ToasterProvider>
      </I18nProvider>
    </ShellProvider>
  );
  const { rerender } = render(wrap(node));
  return { controls, handles, rerender: (child: ReactNode) => rerender(wrap(child)) };
}

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
});
