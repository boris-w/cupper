// @vitest-environment jsdom
import type { ConfirmationRequired, ProjectUpserted } from "@cupper/protocol";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test } from "vitest";

import { I18nProvider, ShellProvider } from "../app/providers";
import { ToasterProvider } from "../feedback/toaster";
import { ElevationBanner } from "../sidebar/parts";
import { ConfirmBanner, UntrustedBanner } from "./SessionBanners";

afterEach(cleanup);

async function setup(node: ReactNode) {
  const { shell, controls } = createBrowserShell();
  const info = await shell.init();
  const wrap = (child: ReactNode) => (
    <ShellProvider api={shell} info={info}>
      <I18nProvider locale="zh-CN">
        <ToasterProvider>{child}</ToasterProvider>
      </I18nProvider>
    </ShellProvider>
  );
  const { rerender } = render(wrap(node));
  return { controls, rerender: (child: ReactNode) => rerender(wrap(child)) };
}

const project = (trust_state: string) =>
  ({ project_id: "p_1", name: "notes", trust_state }) as ProjectUpserted;
const confirmation = (reason: string): ConfirmationRequired => ({ config_digest: "d", reason });
const alerts = () => screen.queryAllByRole("status").map((alert) => alert.textContent);

describe("UNIT-ui-session-banners", () => {
  test("the untrusted box shows for an untrusted or stale project and trusts it", async () => {
    const { controls, rerender } = await setup(<UntrustedBanner project={project("untrusted")} />);
    expect(alerts()).toEqual([expect.stringContaining("项目未信任") as string]);
    fireEvent.click(screen.getByRole("button", { name: "信任此项目" }));
    await act(() => Promise.resolve());
    expect(controls.privilegedActions).toEqual([{ kind: "trust_project", project_id: "p_1" }]);
    rerender(<UntrustedBanner project={project("stale")} />);
    expect(alerts()).toEqual([expect.stringContaining("项目配置有变化") as string]);
    for (const gone of [project("trusted"), undefined]) {
      rerender(<UntrustedBanner project={gone} />);
      expect(alerts()).toEqual([]);
    }
  });

  test("the confirmation box follows confirmation_required and confirms through the shell", async () => {
    const box = (value?: ConfirmationRequired | null) => (
      <ConfirmBanner sessionId="s_1" confirmation={value} />
    );
    const { controls, rerender } = await setup(box(confirmation("full_access_unconfirmed")));
    expect(alerts()).toEqual(["会话处于完全访问，但没有经过确认。确认"]);
    fireEvent.click(screen.getByRole("button", { name: "确认" }));
    await act(() => Promise.resolve());
    expect(controls.privilegedActions).toEqual([{ kind: "confirm_config", session_id: "s_1" }]);
    // Still there until the stream drops it.
    expect(alerts()).toHaveLength(1);
    rerender(box(confirmation("new_reason")));
    expect(alerts()).toEqual(["会话的权限配置需要确认（new_reason）。确认"]);
    rerender(box(null));
    expect(alerts()).toEqual([]);
  });

  test("the elevation banner never shows the requester's own name", async () => {
    const elevation = { elevation_id: "e_1", requested_at: 1, scopes: ["privileged"] };
    await setup(<ElevationBanner elevations={[{ ...elevation, client_name: "系统更新" }]} />);
    expect(alerts()).toEqual(["命令行工具申请权限查看"]);
  });
});
