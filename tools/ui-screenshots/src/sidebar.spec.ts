import type { EventBody } from "@cupper/protocol";
import { expect, test, type Page } from "@playwright/test";

import { open } from "./serve";

const project = (project_id: string, name: string, position: number, trust_state: string) => ({
  type: "project.upserted",
  data: {
    project_id,
    name,
    position,
    trust_state,
    path: `/work/${name}`,
    is_git: true,
    added_at: 1,
  },
});

const session = (session_id: string, title: string, status: string, extra: object = {}) => ({
  type: "session.upserted",
  data: {
    ...{ session_id, title, status, agent: "claude", project_id: "p_1", title_source: "prompt" },
    ...{ last_activity_at: 100 - Number(session_id.slice(2)), pending_interactions: 0 },
    ...{ read_only: false, unread: status === "unread", workspace_kind: "local", ...extra },
  },
});

/** Two projects, a session in each state, and an elevation request from the CLI. */
const INDEX = [
  {
    type: "host.updated",
    data: { version: "0.1.0", pending_upgrade: false, power_assertion: false, weak_auth: false },
  },
  project("p_1", "cupper", 0, "trusted"),
  project("p_2", "notes", 1, "untrusted"),
  session("s_1", "修复登录问题", "running"),
  session("s_2", "补上单元测试", "needs_action", { pending_kind: "approval" }),
  session("s_3", "整理发布说明", "needs_action", { pending_kind: "input" }),
  session("s_4", "迁移配置文件", "error", { error_code: "process_exited" }),
  session("s_5", "更新依赖版本", "unread"),
  session("s_6", "清理旧分支", "idle"),
  session("s_7", "读书笔记", "idle", { project_id: "p_2" }),
  {
    type: "elevation.requested",
    data: {
      elevation_id: "e_1",
      client_name: "cupper-cli",
      scopes: ["privileged"],
      requested_at: 1,
    },
  },
] as EventBody[];

const select = (page: Page) =>
  page.evaluate(() => window.cupperShell.navigate({ kind: "session", session_id: "s_2" }));

for (const theme of ["light", "dark"] as const) {
  test(`UI-sidebar ${theme}`, async ({ page }) => {
    await open(page, { info: { theme }, streams: { index: INDEX } });
    await select(page);
    await expect(page.getByRole("button", { name: /补上单元测试/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(page).toHaveScreenshot(`sidebar-${theme}.png`);
  });
}

test("UI-sidebar narrow: the floating sidebar shows the same content, expanded", async ({
  page,
}) => {
  await page.setViewportSize({ width: 700, height: 820 });
  await open(page, { streams: { index: INDEX } });
  await page.getByRole("button", { name: "切换侧边栏" }).click();
  await page.getByRole("button", { name: "展开显示" }).click();
  await expect(page.getByRole("button", { name: /读书笔记/ })).toBeVisible();
  await page.mouse.move(650, 780); // off the rows, so that none shows its hover colour
  await expect(page).toHaveScreenshot("sidebar-narrow.png");
});

test("UI-banners: elevation, the outcome toasts and the reconnecting state", async ({ page }) => {
  await open(page, { streams: { index: INDEX } });
  await page.evaluate(() =>
    window.cupperShell.setPrivilegedResult({ status: "cooldown", retry_after_ms: 30_000 }),
  );
  await page.getByRole("button", { name: "查看" }).click();
  await page.evaluate(() => window.cupperShell.setPrivilegedResult({ status: "busy" }));
  await page.getByRole("button", { name: "添加项目" }).click();
  // The next handshake never completes: the window stays reconnecting.
  await page.evaluate(() => {
    window.cupperHost.handle("initialize", () => undefined);
    window.cupperHost.connections[0]?.close();
  });
  await expect(page.getByRole("status").filter({ hasText: "正在重新连接会话宿主" })).toBeVisible();
  expect(await page.evaluate(() => window.cupperShell.privilegedActions)).toEqual([
    { kind: "elevation", elevation_id: "e_1" },
    { kind: "add_project" },
  ]);
  await page.mouse.move(650, 780);
  await expect(page).toHaveScreenshot("banners.png");
});

const CONFIGURED = {
  type: "session.configured",
  data: {
    ...{ agent: "claude", config: {}, project_config: "ignored", read_only: false },
    workspace: { kind: "local", path: "/work/notes" },
    confirmation_required: { config_digest: "d", reason: "full_access_unconfirmed" },
  },
} as EventBody;

for (const theme of ["light", "dark"] as const) {
  test(`UI-session-banners ${theme}`, async ({ page }) => {
    await open(page, { info: { theme }, streams: { index: INDEX, "session/s_7": [CONFIGURED] } });
    await page.evaluate(() => window.cupperShell.navigate({ kind: "session", session_id: "s_7" }));
    await expect(page.getByRole("button", { name: "信任此项目" })).toBeVisible();
    await expect(page.getByRole("button", { name: "确认" })).toBeVisible();
    await page.mouse.move(650, 780);
    await expect(page).toHaveScreenshot(`session-banners-${theme}.png`);
  });
}
